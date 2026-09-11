use super::*;
use crate::agent::{AgentProbeRequest, AgentSessionDelete};
use crate::protocol::model::{AgentProbeCapabilities, AgentProbeResult, AgentProbeStatus};
use openaide_app_server_protocol::task::{
    NativeSessionDeleteConfirmation, NativeSessionDeleteParams, NativeSessionDeleteResult,
    NativeSessionDeleteTarget,
};

#[test]
fn confirmed_deletion_removes_the_task_only_after_agent_success() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let workspace = temp.path().join("workspace");
    let mut task = task_record("delete-task", workspace.to_str().unwrap());
    task.agent_session_id = Some("native-delete".into());
    store.write_task(&task).unwrap();
    let agent = Arc::new(DeletionAgent {
        store: Some(store.clone()),
        ..Default::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let result = api
        .delete_native_session_for_test(
            &ClientInstanceId::from("client-a"),
            delete_params("delete-task", Some(false)),
        )
        .unwrap();
    assert!(matches!(result, NativeSessionDeleteResult::Deleted { .. }));
    assert_eq!(agent.deleted.lock().unwrap().as_slice(), ["native-delete"]);
    let error = api
        .open_for_client(
            &ClientInstanceId::from("client-a"),
            TaskOpenParams {
                task_id: TaskId::from("delete-task"),
            },
        )
        .unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::NotFound);
}

#[test]
fn deletion_of_archived_task_removes_saved_history_without_restore() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let mut task = task_record(
        "archived-delete",
        temp.path().join("workspace").to_str().unwrap(),
    );
    task.agent_session_id = Some("archived-session".into());
    task.lifecycle = TaskLifecycle::Archived;
    store.write_task(&task).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(DeletionAgent::default()),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    api.delete_native_session_for_test(
        &ClientInstanceId::from("client-a"),
        delete_params("archived-delete", Some(false)),
    )
    .unwrap();
    let result = api.open_for_client(
        &ClientInstanceId::from("client-a"),
        TaskOpenParams {
            task_id: TaskId::from("archived-delete"),
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
        "archived history must disappear after confirmed deletion"
    );
}

#[test]
fn unknown_deletion_retains_history_and_blocks_work_until_explicit_retry() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let mut task = task_record(
        "unknown-delete",
        temp.path().join("workspace").to_str().unwrap(),
    );
    task.agent_session_id = Some("native-unknown".into());
    store.write_task(&task).unwrap();
    let agent = Arc::new(DeletionAgent {
        failure: Mutex::new(Some(RuntimeError::OutcomeUnknown(
            "deletion outcome is unknown".into(),
        ))),
        ..Default::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let client = ClientInstanceId::from("client-a");
    assert!(api
        .delete_native_session_for_test(&client, delete_params("unknown-delete", Some(false)))
        .is_err());
    // Reload is a real session-work boundary and must reject before attempting Agent recovery.
    let error = api
        .reload_native_session_for_client(
            &client,
            TaskReloadNativeSessionParams {
                task_id: TaskId::from("unknown-delete"),
                client_mutation_id: "reload-after-delete".into(),
            },
        )
        .unwrap_err();
    assert!(
        error.message.contains("deletion outcome is unknown"),
        "{error:?}"
    );
    *agent.failure.lock().unwrap() = None;
    assert!(matches!(
        api.delete_native_session_for_test(&client, delete_params("unknown-delete", Some(false)))
            .unwrap(),
        NativeSessionDeleteResult::Deleted { .. }
    ));
    assert_eq!(
        agent.deleted.lock().unwrap().len(),
        2,
        "only explicit retry dispatches again"
    );
}

fn delete_params(task_id: &str, active: Option<bool>) -> NativeSessionDeleteParams {
    NativeSessionDeleteParams {
        target: NativeSessionDeleteTarget::Task {
            task_id: TaskId::from(task_id),
        },
        confirmation: active.map(|active| NativeSessionDeleteConfirmation {
            active,
            queued_message_count: 0,
        }),
    }
}

#[derive(Default)]
struct DeletionAgent {
    live: Option<RecordingAgent>,
    store: Option<Store>,
    deleted: Mutex<Vec<String>>,
    failure: Mutex<Option<RuntimeError>>,
    unsupported: bool,
}

impl AgentRuntime for DeletionAgent {
    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        self.live
            .as_ref()
            .expect("only live fixture resumes")
            .resume_session(request)
    }
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Ok(AgentProbeResult {
            agent_id: request.agent_id,
            status: AgentProbeStatus::Ready,
            protocol_version: "fixture".into(),
            implementation_name: None,
            implementation_version: None,
            capabilities: Vec::new(),
            typed_capabilities: AgentProbeCapabilities {
                delete_sessions: !self.unsupported,
                ..Default::default()
            },
            auth_methods: Vec::new(),
            logout_supported: false,
        })
    }
    fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        if let Some(store) = &self.store {
            assert!(
                store
                    .list_all_task_records()
                    .unwrap()
                    .iter()
                    .any(|task| !task.tombstoned
                        && task.agent_session_id.as_deref() == Some(&request.session_id)),
                "local history must still exist when the Agent receives Delete"
            );
        }
        self.deleted.lock().unwrap().push(request.session_id);
        self.failure.lock().unwrap().clone().map_or(Ok(()), Err)
    }
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        if let Some(live) = &self.live {
            return live.start_session(_request);
        }
        panic!("deletion must not create a Native Session")
    }
    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        if let Some(live) = &self.live {
            return live.prompt(_prompt, _sink);
        }
        panic!("deletion must not send a prompt")
    }
}

#[test]
fn deletion_rechecks_activity_and_preserves_task_on_agent_rejection() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let mut task = task_record(
        "active-delete",
        temp.path().join("workspace").to_str().unwrap(),
    );
    task.agent_session_id = Some("native-active".into());
    task.status = TaskStatus::Active;
    task.active_turn_id = Some("turn-active".into());
    store.write_task(&task).unwrap();
    let agent = Arc::new(DeletionAgent {
        failure: Mutex::new(Some(RuntimeError::Conflict(
            "Agent refuses active deletion".into(),
        ))),
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
    store.write_task(&task).unwrap(); // Restore live fixture state after startup recovery.
    let client = ClientInstanceId::from("client-a");
    assert!(matches!(
        api.delete_native_session_for_test(&client, delete_params("active-delete", Some(false)))
            .unwrap(),
        NativeSessionDeleteResult::ConfirmationRequired { active: true, .. }
    ));
    assert!(agent.deleted.lock().unwrap().is_empty());
    assert!(api
        .delete_native_session_for_test(&client, delete_params("active-delete", Some(true)))
        .is_err());
    assert!(!store.read_task("active-delete").unwrap().tombstoned);
    assert!(api
        .require_session_not_deleting(&crate::native_sessions::catalog::NativeSessionRef::new(
            "codex",
            "native-active"
        ))
        .is_ok());
}

#[test]
fn unknown_deletion_blocks_automatic_queue_delivery_after_the_current_turn_finishes() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    store
        .write_task(&task_record(
            "queue-unknown",
            temp.path().join("workspace").to_str().unwrap(),
        ))
        .unwrap();
    let agent = Arc::new(DeletionAgent {
        live: Some(RecordingAgent {
            block_prompt: true,
            ..Default::default()
        }),
        failure: Mutex::new(Some(RuntimeError::OutcomeUnknown(
            "delete response lost".into(),
        ))),
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
    api.send(send_params("queue-unknown", "first turn"))
        .unwrap();
    let live = agent.live.as_ref().unwrap();
    wait_until(|| live.prompts.load(Ordering::SeqCst) == 1);
    api.queue_append_for_test(TaskQueueAppendParams {
        task_id: "queue-unknown".into(),
        message: ComposerMessage {
            text: Some("must stay queued".into()),
            ..Default::default()
        },
    })
    .unwrap();
    let mut params = delete_params("queue-unknown", Some(true));
    params.confirmation.as_mut().unwrap().queued_message_count = 1;
    assert!(api
        .delete_native_session_for_test(&ClientInstanceId::from("client-a"), params)
        .is_err());
    live.release_prompt.store(true, Ordering::SeqCst);
    wait_until(|| store.read_task("queue-unknown").unwrap().status == TaskStatus::Inactive);
    assert_eq!(
        live.prompts.load(Ordering::SeqCst),
        1,
        "unknown deletion must not dispatch the next turn"
    );
    assert_eq!(
        store
            .read_task("queue-unknown")
            .unwrap()
            .message_queue
            .items
            .len(),
        1
    );
}

#[test]
fn deletion_uncertainty_blocks_a_previously_accepted_turn_still_acquiring_its_session() {
    assert_pending_startup_cannot_pass_deletion(false);
}

#[test]
fn deletion_uncertainty_blocks_initial_session_replacement_after_acquisition() {
    assert_pending_startup_cannot_pass_deletion(true);
}

fn assert_pending_startup_cannot_pass_deletion(resume_missing: bool) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let mut task = task_record(
        "starting-delete",
        temp.path().join("workspace").to_str().unwrap(),
    );
    task.agent_session_id = Some("starting-native".into());
    store.write_task(&task).unwrap();
    let agent = Arc::new(DeletionAgent {
        live: Some(RecordingAgent {
            block_resume: AtomicBool::new(true),
            resume_session_missing: resume_missing,
            ..Default::default()
        }),
        failure: Mutex::new(Some(RuntimeError::OutcomeUnknown(
            "delete response lost".into(),
        ))),
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
    api.send(send_params("starting-delete", "accepted before delete"))
        .unwrap();
    let live = agent.live.as_ref().unwrap();
    wait_until(|| live.resumes.load(Ordering::SeqCst) == 1);
    assert!(api
        .delete_native_session_for_test(
            &ClientInstanceId::from("client-a"),
            delete_params("starting-delete", Some(true))
        )
        .is_err());
    live.block_resume.store(false, Ordering::SeqCst);
    wait_until(|| {
        store
            .read_task("starting-delete")
            .unwrap()
            .active_turn_id
            .is_none()
    });
    assert_eq!(
        live.prompts.load(Ordering::SeqCst),
        0,
        "pending startup cannot submit a prompt after unresolved Delete"
    );
}

#[test]
fn deleting_an_archived_unadopted_session_never_creates_a_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let agent = Arc::new(DeletionAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let reference =
        crate::native_sessions::catalog::NativeSessionRef::new("codex", "external-delete");
    api.native_catalog
        .record_page(
            "project",
            "/workspace",
            vec![crate::native_sessions::catalog::NativeSessionObservation {
                reference: reference.clone(),
                title: Some("External history".into()),
                last_activity: None,
            }],
        )
        .unwrap();
    api.native_catalog.archive(&reference).unwrap();
    let target = NativeSessionDeleteTarget::NativeSession {
        agent_id: "codex".into(),
        native_session_id: "external-delete".into(),
    };
    let client = ClientInstanceId::from("client-a");
    let preview = api
        .delete_native_session_for_test(
            &client,
            NativeSessionDeleteParams {
                target: target.clone(),
                confirmation: None,
            },
        )
        .unwrap();
    assert!(matches!(
        preview,
        NativeSessionDeleteResult::ConfirmationRequired {
            active: false,
            queued_message_count: 0,
            ..
        }
    ));
    assert!(agent.deleted.lock().unwrap().is_empty());
    api.delete_native_session_for_test(
        &client,
        NativeSessionDeleteParams {
            target,
            confirmation: Some(NativeSessionDeleteConfirmation {
                active: false,
                queued_message_count: 0,
            }),
        },
    )
    .unwrap();
    assert!(api.native_catalog.entry(&reference).is_none());
    assert!(store.list_all_task_records().unwrap().is_empty());
    assert_eq!(
        agent.deleted.lock().unwrap().as_slice(),
        ["external-delete"]
    );
}

#[test]
fn deletion_removes_only_its_composer_history_contribution_and_preserves_project_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let workspace = temp.path().join("workspace");
    let project_id = project_id_for_workspace(workspace.to_str().unwrap());
    for id in ["delete-history", "keep-history"] {
        let mut task = task_record(id, workspace.to_str().unwrap());
        task.agent_session_id = Some(format!("native-{id}"));
        for text in if id == "delete-history" {
            vec!["shared", "deleted-only"]
        } else {
            vec!["shared"]
        } {
            task.composer_history.record(ComposerHistoryEntryRecord {
                entry_id: format!("{id}-{text}"),
                project_id: project_id.as_str().into(),
                text: text.into(),
                accepted_at: "100".into(),
            });
        }
        store.write_task(&task).unwrap();
    }
    let referenced = workspace.join("keep.txt");
    std::fs::write(&referenced, "project-owned content").unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(DeletionAgent::default()),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let client = ClientInstanceId::from("client-a");
    api.delete_native_session_for_test(&client, delete_params("delete-history", Some(false)))
        .unwrap();
    let history = api
        .composer_history_for_client(
            &client,
            ComposerHistoryParams {
                scope: ComposerHistoryScope::Project { project_id },
            },
        )
        .unwrap();
    assert_eq!(
        history
            .entries
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>(),
        ["shared"]
    );
    assert_eq!(
        std::fs::read_to_string(referenced).unwrap(),
        "project-owned content"
    );
}

#[test]
fn deletion_diagnostics_include_target_resolution_failure() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(DeletionAgent::default()),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let logs = crate::logging::capture_test_logs();
    assert!(api
        .delete_native_session_for_test(
            &ClientInstanceId::from("client-a"),
            delete_params("missing-delete-diagnostics", Some(false)),
        )
        .is_err());
    let events = logs
        .snapshot()
        .into_iter()
        .filter(|line| line["fields"]["task_id"] == "missing-delete-diagnostics")
        .map(|line| line["event"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|event| event == "native_session_delete_started"));
    assert!(events
        .iter()
        .any(|event| event == "native_session_delete_completed"));
}

impl TaskProductApi {
    fn delete_native_session_for_test(
        &self,
        client: &ClientInstanceId,
        params: NativeSessionDeleteParams,
    ) -> Result<NativeSessionDeleteResult, ProtocolError> {
        self.delete_native_session_for_client(client, params, &uuid::Uuid::new_v4().to_string())
    }
}
