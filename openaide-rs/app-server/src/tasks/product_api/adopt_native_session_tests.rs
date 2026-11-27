//! Native Session adoption runs for as long as the Agent takes to `session/load`; it must
//! not hold the shared Native Session gate across that wait, and concurrent requests for one
//! session must converge on one Task.

use super::*;
use crate::agent::events::{
    AgentNativeSubagentCapabilities, AgentNativeSubagentSpawned, AgentNativeSubagentState,
    AgentNativeSubagentStateUpdate, AgentReplayedSubagentEvent,
};
use crate::tasks::mutation::TaskCommitOptions;

const WORKSPACE: &str = "/tmp/openaide-unit-workspace/adopt";

struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    agent: Arc<RecordingAgent>,
    api: TaskProductApi,
    updates: crate::task_events::TaskUpdateReceiver,
    /// Subagent restores the Task runner handed to its background worker, not yet run.
    background: Arc<Mutex<Vec<crate::tasks::turns::BackgroundJob>>>,
    /// When set, the background worker cannot start.
    refuse_background: Arc<AtomicBool>,
}

fn listed(session_id: &str) -> AgentListedSession {
    AgentListedSession {
        session_id: session_id.to_string(),
        cwd: WORKSPACE.to_string(),
        title: Some(format!("Session {session_id}")),
        updated_at: None,
        last_activity: None,
    }
}

fn fixture(session_ids: &[&str]) -> Fixture {
    fixture_with(session_ids, Vec::new(), true)
}

fn fixture_with(
    session_ids: &[&str],
    replayed_subagents: Vec<AgentReplayedSubagentEvent>,
    block_load: bool,
) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("project-task", WORKSPACE))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(session_ids.iter().map(|id| listed(id)).collect()),
        block_load: AtomicBool::new(block_load),
        replayed_subagents: Mutex::new(replayed_subagents),
        ..RecordingAgent::default()
    });
    let (notifier, updates) = TaskUpdateNotifier::channel();
    let mut api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        notifier,
    )
    .unwrap();
    let background = Arc::new(Mutex::new(Vec::new()));
    let refuse_background = Arc::new(AtomicBool::new(false));
    let (queue, refuse) = (background.clone(), refuse_background.clone());
    api.turn_runner =
        api.turn_runner
            .clone()
            .with_background_spawner(Arc::new(move |_name, job| {
                if refuse.load(Ordering::SeqCst) {
                    return Err(RuntimeError::Internal("no worker".to_string()));
                }
                queue.lock().unwrap().push(job);
                Ok(())
            }));
    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace(WORKSPACE),
        cursor: None,
    })
    .expect("discover sessions");
    Fixture {
        _temp: temp,
        store,
        agent,
        api,
        updates,
        background,
        refuse_background,
    }
}

fn params(session_id: &str) -> TaskAdoptNativeSessionParams {
    TaskAdoptNativeSessionParams {
        agent_id: AgentId::from("codex"),
        native_session_id: session_id.to_string(),
    }
}

fn adopting(
    api: &TaskProductApi,
    session_id: &str,
) -> std::thread::JoinHandle<Result<TaskSnapshot, ProtocolError>> {
    let api = api.clone();
    let params = params(session_id);
    std::thread::spawn(move || api.adopt_native_session(params))
}

#[test]
fn a_slow_session_load_does_not_hold_the_native_session_gate() {
    let fixture = fixture(&["slow", "other"]);
    let adoption = adopting(&fixture.api, "slow");
    crate::test_sync::wait_until("the Agent starts loading", || {
        fixture.agent.loads.load(Ordering::SeqCst) == 1
    });

    // Every request that takes the gate completes while the load is still blocked.
    let api = fixture.api.clone();
    let metadata =
        std::thread::spawn(move || api.set_native_session_pinned("codex", "other", true));
    crate::test_sync::wait_until("a gate user finishes during the load", || {
        metadata.is_finished()
    });
    metadata
        .join()
        .unwrap()
        .expect("pinning another Native Session while one loads");

    fixture.agent.block_load.store(false, Ordering::SeqCst);
    adoption.join().unwrap().expect("adoption completes");
}

#[test]
fn concurrent_adoptions_of_one_session_create_one_task() {
    let fixture = fixture(&["shared"]);
    let first = adopting(&fixture.api, "shared");
    crate::test_sync::wait_until("the first adoption loads", || {
        fixture.agent.loads.load(Ordering::SeqCst) == 1
    });
    let second = adopting(&fixture.api, "shared");
    // The second request is held behind the first one's keyed gate: it never reaches the
    // Agent while the first load is blocked.
    crate::test_sync::observe_absence();
    assert_eq!(fixture.agent.loads.load(Ordering::SeqCst), 1);

    fixture.agent.block_load.store(false, Ordering::SeqCst);
    let first = first.join().unwrap().expect("first adoption");
    let second = second
        .join()
        .unwrap()
        .expect("second adoption joins the first");

    assert_eq!(first.task.task_id, second.task.task_id);
    assert_eq!(fixture.agent.loads.load(Ordering::SeqCst), 1);
    let owners = fixture
        .store
        .list_all_task_records_strict()
        .unwrap()
        .into_iter()
        .filter(|task| task.agent_session_id.as_deref() == Some("shared"))
        .count();
    assert_eq!(owners, 1);
}

#[test]
fn a_session_removed_while_loading_is_not_adopted() {
    let fixture = fixture(&["vanishing"]);
    let adoption = adopting(&fixture.api, "vanishing");
    crate::test_sync::wait_until("the Agent starts loading", || {
        fixture.agent.loads.load(Ordering::SeqCst) == 1
    });
    fixture
        .api
        .native_catalog
        .remove(&crate::native_sessions::catalog::NativeSessionRef::new(
            "codex",
            "vanishing",
        ))
        .unwrap();

    fixture.agent.block_load.store(false, Ordering::SeqCst);
    let error = adoption
        .join()
        .unwrap()
        .expect_err("the commit re-checks the catalog");
    assert_eq!(error.code, ProtocolErrorCode::NotFound);
    assert!(fixture
        .store
        .list_all_task_records_strict()
        .unwrap()
        .iter()
        .all(|task| task.agent_session_id.as_deref() != Some("vanishing")));
    // The session the Agent opened for the abandoned adoption is closed, not leaked.
    assert_eq!(fixture.agent.closes.load(Ordering::SeqCst), 1);
}

fn spawned(child: &str, parent: &str) -> AgentReplayedSubagentEvent {
    AgentReplayedSubagentEvent::Spawned(AgentNativeSubagentSpawned {
        parent_native_session_id: parent.to_string(),
        native_session_id: child.to_string(),
        name: "Reviewer".to_string(),
        delegated_task: Some("Summary".to_string()),
        capabilities: AgentNativeSubagentCapabilities::default(),
        details: Vec::new(),
    })
}

fn completed(child: &str, parent: &str) -> AgentReplayedSubagentEvent {
    AgentReplayedSubagentEvent::State(AgentNativeSubagentStateUpdate {
        parent_native_session_id: parent.to_string(),
        native_session_id: child.to_string(),
        state: AgentNativeSubagentState::Completed,
    })
}

fn run_background(fixture: &Fixture) {
    let jobs = std::mem::take(&mut *fixture.background.lock().unwrap());
    for job in jobs {
        job();
    }
}

fn logged(logs: &[serde_json::Value], event: &str, task_id: &str) -> bool {
    logs.iter()
        .any(|entry| entry["event"] == event && entry["fields"]["task_id"] == task_id)
}

#[test]
fn the_reply_does_not_wait_for_replayed_subagents_and_a_subscriber_then_sees_them() {
    let fixture = fixture_with(
        &["parented"],
        vec![spawned("child", "parented"), completed("child", "parented")],
        false,
    );

    // The worker is parked: the adoption already replied with the committed Task.
    let snapshot = fixture
        .api
        .adopt_native_session(params("parented"))
        .expect("adoption replies");
    let task_id = snapshot.task.task_id.as_str().to_string();
    assert_eq!(fixture.background.lock().unwrap().len(), 1);
    assert!(fixture
        .store
        .subagent_record_by_native(&task_id, "child")
        .is_err());
    while fixture.updates.try_recv().is_ok() {}

    run_background(&fixture);

    let record = fixture
        .store
        .subagent_record_by_native(&task_id, "child")
        .expect("the restored child");
    assert_eq!(
        record.status,
        crate::storage::subagents::SubagentStatusRecord::Completed
    );
    // The subscriber learns of it through the ordinary catalog notification.
    crate::test_sync::wait_until("the Subagent catalog is published", || {
        std::iter::from_fn(|| fixture.updates.try_recv().ok()).any(|update| {
            update.task_id == task_id
                && matches!(
                    update.kind,
                    crate::task_events::TaskUpdateKind::SubagentCatalogChanged { .. }
                )
        })
    });
}

#[test]
fn a_second_adoption_does_not_restore_the_replayed_subagents_again() {
    let fixture = fixture_with(
        &["twice"],
        vec![spawned("child", "twice"), completed("child", "twice")],
        false,
    );
    let first = fixture.api.adopt_native_session(params("twice")).unwrap();
    let second = fixture.api.adopt_native_session(params("twice")).unwrap();

    assert_eq!(first.task.task_id, second.task.task_id);
    assert_eq!(fixture.background.lock().unwrap().len(), 1);
}

#[test]
fn a_task_removed_before_the_restore_runs_is_skipped() {
    let logs = crate::logging::capture_test_logs();
    let fixture = fixture_with(
        &["removed"],
        vec![spawned("child", "removed"), completed("child", "removed")],
        false,
    );
    let snapshot = fixture.api.adopt_native_session(params("removed")).unwrap();
    let task_id = snapshot.task.task_id.as_str().to_string();
    fixture
        .api
        .mutations
        .commit_existing_task(&task_id, TaskCommitOptions::metadata(), |ctx| {
            ctx.task_mut().tombstoned = true;
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();

    run_background(&fixture);

    assert!(fixture
        .store
        .subagent_record_by_native(&task_id, "child")
        .is_err());
    assert!(logged(
        &logs.snapshot(),
        "task_subagent_replay_restore_skipped",
        &task_id
    ));
}

#[test]
fn a_failing_child_is_logged_and_never_fails_the_task() {
    let logs = crate::logging::capture_test_logs();
    // A child without a parent id cannot be recorded.
    let fixture = fixture_with(
        &["broken"],
        vec![
            spawned("orphan", ""),
            spawned("child", "broken"),
            completed("child", "broken"),
        ],
        false,
    );
    let snapshot = fixture.api.adopt_native_session(params("broken")).unwrap();
    let task_id = snapshot.task.task_id.as_str().to_string();

    run_background(&fixture);

    let entries = logs.snapshot();
    assert!(logged(
        &entries,
        "task_subagent_replay_child_failed",
        &task_id
    ));
    assert!(logged(
        &entries,
        "task_subagent_replay_restore_finished",
        &task_id
    ));
    assert!(fixture
        .store
        .subagent_record_by_native(&task_id, "child")
        .is_ok());
    assert!(fixture.store.read_task(&task_id).is_ok());
}

#[test]
fn a_restore_that_cannot_start_is_logged_and_the_adoption_still_succeeds() {
    let logs = crate::logging::capture_test_logs();
    let fixture = fixture_with(
        &["undispatched"],
        vec![spawned("child", "undispatched")],
        false,
    );
    fixture.refuse_background.store(true, Ordering::SeqCst);

    let snapshot = fixture
        .api
        .adopt_native_session(params("undispatched"))
        .expect("adoption succeeds without the restore");

    assert!(logged(
        &logs.snapshot(),
        "task_subagent_replay_restore_dispatch_failed",
        snapshot.task.task_id.as_str()
    ));
}
