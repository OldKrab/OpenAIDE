//! Native Session adoption runs for as long as the Agent takes to `session/load`; it must
//! not hold the shared Native Session gate across that wait, and concurrent requests for one
//! session must converge on one Task.

use super::*;

const WORKSPACE: &str = "/tmp/openaide-unit-workspace/adopt";

struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    agent: Arc<RecordingAgent>,
    api: TaskProductApi,
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
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("project-task", WORKSPACE))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(session_ids.iter().map(|id| listed(id)).collect()),
        block_load: AtomicBool::new(true),
        ..RecordingAgent::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
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
