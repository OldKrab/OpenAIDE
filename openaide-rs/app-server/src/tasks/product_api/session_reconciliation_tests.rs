use super::*;
use crate::native_sessions::catalog::{NativeSessionObservation, NativeSessionRef};

#[test]
fn complete_history_refresh_removes_missing_open_archived_and_unadopted_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let workspace = temp.path().join("workspace");
    for (id, lifecycle) in [
        ("open-missing", TaskLifecycle::Open),
        ("archived-missing", TaskLifecycle::Archived),
    ] {
        let mut task = task_record(id, workspace.to_str().unwrap());
        task.agent_session_id = Some(format!("native-{id}"));
        task.lifecycle = lifecycle;
        store.write_task(&task).unwrap();
    }
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(RecordingAgent::default()),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let reference = NativeSessionRef::new("codex", "unadopted-missing");
    api.native_session_catalog()
        .record_page(
            project_id_for_workspace(workspace.to_str().unwrap()).as_str(),
            workspace.to_str().unwrap(),
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: Some("Missing".into()),
                last_activity: None,
            }],
        )
        .unwrap();
    api.refresh_native_session_catalogs().unwrap();
    assert!(api.native_session_catalog().entry(&reference).is_none());
    for id in ["open-missing", "archived-missing"] {
        let result = api.open_for_client(
            &ClientInstanceId::from("client-a"),
            TaskOpenParams {
                task_id: TaskId::from(id),
            },
        );
        assert!(
            matches!(
                result,
                Err(ProtocolError {
                    code: ProtocolErrorCode::NotFound,
                    ..
                })
            ),
            "{id} should be removed after authoritative absence"
        );
    }
}

#[test]
fn a_newer_identical_catalog_observation_survives_an_older_complete_scan() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let workspace = temp.path().join("workspace");
    store
        .write_task(&task_record(
            "workspace-anchor",
            workspace.to_str().unwrap(),
        ))
        .unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(BlockingHistoryAgent {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let reference = NativeSessionRef::new("codex", "still-present");
    let observation = NativeSessionObservation {
        reference: reference.clone(),
        title: Some("Unchanged title".into()),
        last_activity: None,
    };
    let catalog = api.native_session_catalog();
    let record = || {
        catalog
            .record_page(
                project_id_for_workspace(workspace.to_str().unwrap()).as_str(),
                workspace.to_str().unwrap(),
                vec![observation.clone()],
            )
            .unwrap()
    };
    record();
    let worker = std::thread::spawn({
        let api = api.clone();
        move || api.refresh_native_session_catalogs()
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    record(); // A newer response proved existence even though all metadata is identical.
    release_tx.send(()).unwrap();
    worker.join().unwrap().unwrap();
    assert!(
        catalog.entry(&reference).is_some(),
        "older absence cannot erase newer positive evidence"
    );
}

struct BlockingHistoryAgent {
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl AgentRuntime for BlockingHistoryAgent {
    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        if request.agent_id == "codex" {
            self.entered.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
        }
        Ok(AgentListSessionsResult {
            agent_id: request.agent_id,
            sessions: Vec::new(),
            next_cursor: None,
        })
    }
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        panic!("listing does not start sessions")
    }
    fn prompt(
        &self,
        _request: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        panic!("listing does not prompt")
    }
}

#[test]
fn definitive_missing_session_on_open_removes_history_without_listing() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let mut task = task_record(
        "missing-on-open",
        temp.path().join("workspace").to_str().unwrap(),
    );
    task.agent_session_id = Some("missing-native".into());
    store.write_task(&task).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(MissingHistoryAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let client = ClientInstanceId::from("client-a");
    let params = TaskOpenParams {
        task_id: TaskId::from("missing-on-open"),
    };
    api.open_for_client(&client, params.clone()).unwrap();
    wait_until(|| {
        matches!(
            api.open_for_client(&client, params.clone()),
            Err(ProtocolError {
                code: ProtocolErrorCode::NotFound,
                ..
            })
        )
    });
}

struct MissingHistoryAgent;
impl AgentRuntime for MissingHistoryAgent {
    fn resume_session(&self, _request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        Err(RuntimeError::NativeSessionMissing(
            "Session not found".into(),
        ))
    }
    fn list_sessions(
        &self,
        _request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        panic!("recovery adds no listing request")
    }
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        panic!("missing established history is not replaced")
    }
    fn prompt(
        &self,
        _request: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        panic!("opening does not prompt")
    }
}

#[test]
fn changing_the_agent_catalog_invalidates_in_flight_absence_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let workspace = temp.path().join("workspace");
    let mut task = task_record("changed-scope", workspace.to_str().unwrap());
    task.agent_session_id = Some("preserve-native".into());
    store.write_task(&task).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(BlockingHistoryAgent {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let worker = std::thread::spawn({
        let api = api.clone();
        move || api.refresh_native_session_catalogs()
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    api.agent_registry
        .replace(AgentRegistry::default_built_ins());
    release_tx.send(()).unwrap();
    worker.join().unwrap().unwrap();
    assert!(
        !store
            .read_task("changed-scope")
            .expect("changed scope must retain history")
            .tombstoned
    );
}

#[test]
fn incomplete_listing_never_removes_missing_history() {
    for scenario in ["bounded", "failed", "cyclic", "no-progress"] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("state")).unwrap();
        let workspace = temp.path().join("workspace");
        let mut task = task_record("preserved-history", workspace.to_str().unwrap());
        task.agent_session_id = Some("not-in-pages".into());
        task.lifecycle = TaskLifecycle::Archived;
        store.write_task(&task).unwrap();
        let api = TaskProductApi::new(
            store.clone(),
            Arc::new(StorageProjectResolver::new(store.clone())),
            AgentRegistry::default_built_ins(),
            Arc::new(IncompleteHistoryAgent(scenario)),
            TaskUpdateNotifier::disabled(),
        )
        .unwrap();
        let reference = NativeSessionRef::new("codex", "not-in-pages");
        api.native_session_catalog()
            .record_page(
                project_id_for_workspace(workspace.to_str().unwrap()).as_str(),
                workspace.to_str().unwrap(),
                vec![NativeSessionObservation {
                    reference: reference.clone(),
                    title: None,
                    last_activity: None,
                }],
            )
            .unwrap();
        let result = api.refresh_native_session_project_trees(
            None,
            Some(if scenario == "bounded" { 1 } else { 20 }),
        );
        assert_eq!(result.is_err(), scenario == "failed");
        assert!(
            !store.read_task("preserved-history").unwrap().tombstoned,
            "{scenario}"
        );
        assert!(
            api.native_session_catalog().entry(&reference).is_some(),
            "{scenario}"
        );
    }
}

struct IncompleteHistoryAgent(&'static str);
impl AgentRuntime for IncompleteHistoryAgent {
    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        if request.agent_id != "codex" {
            return Ok(AgentListSessionsResult {
                agent_id: request.agent_id,
                sessions: vec![],
                next_cursor: None,
            });
        }
        if self.0 == "failed" && request.cursor.is_some() {
            return Err(RuntimeError::Internal("listing failed".into()));
        }
        let second = request.cursor.is_some();
        let id = if self.0 == "no-progress" || !second {
            "observed-a"
        } else {
            "observed-b"
        };
        Ok(AgentListSessionsResult {
            agent_id: request.agent_id,
            sessions: vec![AgentListedSession {
                session_id: id.into(),
                cwd: request.cwd.unwrap(),
                title: None,
                last_activity: None,
                updated_at: None,
            }],
            next_cursor: Some("same-cursor".into()),
        })
    }
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        panic!("listing never starts work")
    }
    fn prompt(
        &self,
        _request: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        panic!("listing never prompts")
    }
}

#[test]
fn listing_absence_does_not_remove_a_task_while_its_turn_is_live() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    store
        .write_task(&task_record(
            "live-history",
            temp.path().join("workspace").to_str().unwrap(),
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_prompt: true,
        ..Default::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    api.send(send_params("live-history", "first turn")).unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    api.refresh_native_session_catalogs().unwrap();
    let retained = store.read_task("live-history");
    agent.release_prompt.store(true, Ordering::SeqCst);
    assert!(
        !retained
            .expect("an empty listing must not erase active work")
            .tombstoned
    );
    wait_until(|| store.read_task("live-history").unwrap().status == TaskStatus::Inactive);
}
