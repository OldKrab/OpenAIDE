use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use openaide_app_server_protocol::agent::AgentListSessionsParams;
use openaide_app_server_protocol::ids::{AgentId, ClientInstanceId, ProjectId, TaskId};
use openaide_app_server_protocol::snapshot::{
    LiveSessionDataState, MessagePart, TaskPreparationSnapshot, TaskSendCapabilityState,
};
use openaide_app_server_protocol::support::SupportRecoverStuckSessionsParams;
use openaide_app_server_protocol::task::{
    ComposerMessage, TaskCancelParams, TaskCreateParams, TaskDiscardParams, TaskMarkReadParams,
    TaskOpenParams, TaskSendParams, TaskSetArchivedParams, TaskSetConfigOptionParams,
};
use openaide_app_server_protocol::workspace::WorkspaceListDirectoryParams;

use crate::agent::registry::{AgentCatalogRecord, AgentRegistry};
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentEventSink, AgentListSessionsRequest, AgentLoadedSession, AgentPrompt, AgentRuntime,
    AgentSession, AgentSessionEventSink, AgentSessionLoad, AgentSessionResume,
    AgentSessionSetConfigOptionRequest, AgentSessionStart,
};
use crate::client_lifecycle::{AppServerTime, ConnectionId, Delivery};
use crate::projects::{project_id_for_workspace, StorageProjectResolver};
use crate::protocol::model::{
    ActivityStatus, ActivityStep, AgentCommand, AgentCommandsCatalog, AgentListSessionsResult,
    AgentListedSession, ChatMessage, ConfigOption, ConfigOptionCategory, ConfigOptionValue,
    ConfigOptionsCatalog, ConfigOptionsStatus, InterruptionReason, IsolationKind,
    NormalizedMessage, TaskStatus,
};
use crate::server_requests::{ServerRequestAnswer, ServerRequestRuntime};
use crate::snapshots::task_snapshot::project_stored_task_snapshot;
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::send_receipts::TaskSendReceipt;
use crate::storage::Store;
use crate::task_events::TaskUpdateNotifier;

use super::*;

#[test]
fn create_persists_idle_task_without_prompt_or_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.first_prompt_sent = false;
    store.write_task(&record).unwrap();
    let project_id = project_id_for_workspace("/workspace/app");
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = api
        .create(TaskCreateParams {
            project_id,
            agent_id: AgentId::from("codex"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap();

    let record = store.read_task(snapshot.task.task_id.as_str()).unwrap();
    assert_eq!(record.status, TaskStatus::Inactive);
    assert!(!record.first_prompt_sent);
    assert_eq!(record.active_turn_id, None);
    assert!(store.read_messages(&record.task_id).unwrap().is_empty());
    assert_eq!(snapshot.chat.items.len(), 0);
}

#[test]
fn create_reopens_the_existing_draft_for_the_same_project_and_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut project_anchor = task_record("task-project-anchor", "/workspace/app");
    project_anchor.first_prompt_sent = true;
    store.write_task(&project_anchor).unwrap();
    let project_id = project_id_for_workspace("/workspace/app");
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let params = TaskCreateParams {
        project_id,
        agent_id: AgentId::from("codex"),
        workspace_root: None,
        config_options: Default::default(),
    };

    let first = api.create(params.clone()).unwrap();
    let reopened = api.create(params).unwrap();

    assert_eq!(reopened.task.task_id, first.task.task_id);
    assert_eq!(store.task_record_count().unwrap(), 2);
}

#[test]
fn create_reactivates_the_reused_draft_native_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/workspace/app");
    draft.first_prompt_sent = false;
    draft.agent_session_id = Some("session-draft".to_string());
    draft.preparation = TaskPreparationRecord::Ready;
    store.write_task(&draft).unwrap();
    let agent = Arc::new(RecordingAgent {
        config_catalog: Some(config_catalog("gpt-5")),
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

    api.create(TaskCreateParams {
        project_id: project_id_for_workspace("/workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
        config_options: Default::default(),
    })
    .unwrap();

    wait_until(|| agent.resumes.load(Ordering::SeqCst) == 1);
    wait_until(|| {
        matches!(
            store.read_task("task-draft").unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });
    assert_eq!(
        store
            .read_task("task-draft")
            .unwrap()
            .agent_session_id
            .as_deref(),
        Some("session-draft")
    );
    let updated = TaskSetConfigOptionWorkflow::set_config_option(
        &api,
        TaskSetConfigOptionParams {
            task_id: "task-draft".into(),
            config_id: "model".into(),
            value: "gpt-5.6-terra".to_string(),
            client_mutation_id: "reactivated-draft-config".into(),
        },
    )
    .unwrap();
    assert_eq!(
        updated.agent_config.options[0].current_value,
        "gpt-5.6-terra"
    );
}

#[test]
fn create_passes_selected_config_into_draft_session_preparation() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.create(TaskCreateParams {
        project_id: project_id_for_workspace("/workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
        config_options: [("model".to_string(), "gpt-5".to_string())]
            .into_iter()
            .collect(),
    })
    .unwrap();

    wait_until(|| !agent.start_config_options.lock().unwrap().is_empty());
    assert_eq!(
        agent.start_config_options.lock().unwrap().as_slice(),
        &[Some(serde_json::json!({ "model": "gpt-5" }))]
    );
}

#[test]
fn shutdown_marks_storage_clean_after_task_runtime_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.shutdown().unwrap();

    let marker_path = temp.path().join(".openaide-runtime/storage-state.json");
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(marker_path).unwrap()).unwrap();
    assert_eq!(marker["state"], "clean");
}

#[test]
fn startup_recovers_active_turn_left_by_previous_product_api_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-stale-turn", "/workspace/app");
    record.status = TaskStatus::Active;
    record.active_turn_id = Some("turn-stale".to_string());
    record.agent_session_id = Some("session-stale".to_string());
    store.write_task(&record).unwrap();
    append_running_turn(&store, "task-stale-turn", "turn-stale");

    TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let recovered = store.read_task("task-stale-turn").unwrap();
    assert_eq!(recovered.status, TaskStatus::Inactive);
    assert_eq!(recovered.active_turn_id, None);
    assert_eq!(recovered.agent_session_id.as_deref(), Some("session-stale"));

    let messages = store.read_messages("task-stale-turn").unwrap();
    assert!(messages.iter().any(|message| {
        matches!(
            message.chat.message,
            NormalizedMessage::Activity {
                status: ActivityStatus::Completed,
                ..
            }
        )
    }));
    assert!(messages.iter().any(|message| {
        matches!(
            message.chat.message,
            NormalizedMessage::Interruption {
                reason: InterruptionReason::Canceled,
                recoverable: true,
                ..
            }
        )
    }));
}

#[test]
fn create_persists_preparing_and_starts_one_native_session_asynchronously() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        config_catalog: Some(config_catalog("gpt-5")),
        commands_catalog: Some(command_catalog()),
        ..RecordingAgent::default()
    });
    agent.block_start.store(true, Ordering::SeqCst);
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace("/workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap();

    wait_until(|| agent.starts.load(Ordering::SeqCst) == 1);
    let preparing_record = store.read_task(snapshot.task.task_id.as_str()).unwrap();
    let accepted = api
        .send(send_params(
            snapshot.task.task_id.as_str(),
            snapshot.revision,
            "send-before-ready",
            "too soon",
        ))
        .unwrap();
    agent.block_start.store(false, Ordering::SeqCst);

    assert!(matches!(
        snapshot.preparation,
        TaskPreparationSnapshot::Preparing { .. }
    ));
    assert_eq!(snapshot.agent_config.state, LiveSessionDataState::Loading);
    assert_eq!(
        snapshot.send_capability.state,
        TaskSendCapabilityState::Loading
    );
    assert!(matches!(
        preparing_record.preparation,
        TaskPreparationRecord::Preparing
    ));
    assert_eq!(preparing_record.agent_session_id, None);
    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    assert!(store
        .read_messages(snapshot.task.task_id.as_str())
        .unwrap()
        .iter()
        .any(|message| matches!(message.chat.message, NormalizedMessage::User { .. })));

    wait_until(|| {
        matches!(
            store
                .read_task(snapshot.task.task_id.as_str())
                .unwrap()
                .preparation,
            TaskPreparationRecord::Ready
        )
    });
    let ready = api
        .open(TaskOpenParams {
            task_id: snapshot.task.task_id.clone(),
        })
        .unwrap();

    assert!(matches!(ready.preparation, TaskPreparationSnapshot::Ready));
    assert_eq!(ready.agent_config.state, LiveSessionDataState::Ready);
    assert_eq!(ready.agent_config.options[0].current_value, "gpt-5");
    assert_eq!(ready.agent_commands.state, LiveSessionDataState::Ready);
    assert_eq!(ready.agent_commands.commands[0].name, "web");
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 2);
    assert_eq!(
        store
            .read_task(snapshot.task.task_id.as_str())
            .unwrap()
            .agent_session_id,
        Some("recorded-session".to_string())
    );
}

#[test]
fn create_projects_native_session_start_failure_into_the_accepted_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        fail_start: true,
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

    let created = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace("/workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap();

    wait_until(|| {
        matches!(
            store
                .read_task(created.task.task_id.as_str())
                .unwrap()
                .preparation,
            TaskPreparationRecord::Failed { .. }
        )
    });
    let failed_record = store.read_task(created.task.task_id.as_str()).unwrap();
    let failed = api
        .open(TaskOpenParams {
            task_id: created.task.task_id.clone(),
        })
        .unwrap();
    let accepted = api
        .send(send_params(
            created.task.task_id.as_str(),
            failed.revision,
            "send-after-failure",
            "do not commit",
        ))
        .unwrap();

    wait_until(|| {
        store
            .read_task(created.task.task_id.as_str())
            .map(|task| task.status == TaskStatus::Failed)
            .unwrap_or(false)
    });

    assert!(matches!(
        failed.preparation,
        TaskPreparationSnapshot::Failed { .. }
    ));
    assert_eq!(failed.agent_config.state, LiveSessionDataState::Failed);
    assert_eq!(
        failed.send_capability.state,
        TaskSendCapabilityState::Failed
    );
    assert!(matches!(
        failed_record.preparation,
        TaskPreparationRecord::Failed { ref message }
            if message.contains("agent failed to start")
    ));
    assert_eq!(failed_record.agent_session_id, None);
    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    assert!(store
        .read_messages(created.task.task_id.as_str())
        .unwrap()
        .iter()
        .any(|message| matches!(message.chat.message, NormalizedMessage::User { .. })));
}

#[test]
fn task_preparation_resolves_custom_agent_secrets_through_typed_server_requests() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let resolved = Arc::new(Mutex::new(None));
    let agent = Arc::new(SecretResolvingAgent {
        resolved: resolved.clone(),
    });
    let registry = AgentRegistry::from_agent_catalog(vec![AgentCatalogRecord::custom(
        "custom.agent".to_string(),
        "Custom Agent".to_string(),
        "C".to_string(),
        true,
        "custom-agent".to_string(),
        "custom-agent".to_string(),
        Vec::new(),
        HashMap::new(),
        vec!["TOKEN".to_string()],
    )])
    .unwrap();
    let server_requests = ServerRequestRuntime::new();
    let api = TaskProductApi::new_with_server_requests(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        registry,
        agent,
        TaskUpdateNotifier::disabled(),
        server_requests.clone(),
    )
    .unwrap();

    let created = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace("/workspace/app"),
            agent_id: AgentId::from("custom.agent"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap();
    let task_id = created.task.task_id.clone();
    wait_until(|| !server_requests.pending_for_task(&task_id).is_empty());
    let delivery = Delivery {
        client_instance_id: ClientInstanceId::from("client-1"),
        connection_id: ConnectionId::new("connection-1"),
        request_capabilities: vec![crate::client_lifecycle::RequestCapability::Permission],
    };
    let deliveries = server_requests.observe_subscription_added(
        delivery.clone(),
        task_id.clone(),
        AppServerTime::now(),
    );
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].envelope.method, "secret/read");
    assert_eq!(
        deliveries[0].envelope.params["key"],
        "openaide.agent.custom.agent.env.TOKEN"
    );
    server_requests.handle_response(
        delivery.client_instance_id,
        deliveries[0].envelope.request_id.clone(),
        ServerRequestAnswer::Result(serde_json::json!({ "value": "resolved-secret" })),
        AppServerTime::now(),
    );

    wait_until(|| {
        matches!(
            store.read_task(task_id.as_str()).unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });
    assert_eq!(
        resolved.lock().unwrap().as_ref(),
        Some(&HashMap::from([(
            "TOKEN".to_string(),
            "resolved-secret".to_string()
        )]))
    );
}

#[test]
fn create_closes_native_session_when_preparation_event_attachment_fails() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        fail_attach: true,
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

    let created = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace("/workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap();

    wait_until(|| {
        matches!(
            store
                .read_task(created.task.task_id.as_str())
                .unwrap()
                .preparation,
            TaskPreparationRecord::Failed { .. }
        )
    });
    let failed = store.read_task(created.task.task_id.as_str()).unwrap();

    assert_eq!(failed.agent_session_id, None);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(agent.closes.load(Ordering::SeqCst), 1);
}

#[test]
fn first_send_reuses_the_native_session_prepared_during_create() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let created = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace("/workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap();
    wait_until(|| {
        matches!(
            store
                .read_task(created.task.task_id.as_str())
                .unwrap()
                .preparation,
            TaskPreparationRecord::Ready
        )
    });
    let ready = store.read_task(created.task.task_id.as_str()).unwrap();

    api.send(send_params(
        created.task.task_id.as_str(),
        ready.revision,
        "first-send",
        "hello",
    ))
    .unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        agent.prompt_calls.lock().unwrap().as_slice(),
        &[("recorded-session".to_string(), "hello".to_string())]
    );
    assert_eq!(
        store
            .read_task(created.task.task_id.as_str())
            .unwrap()
            .agent_session_id
            .as_deref(),
        Some("recorded-session")
    );
}

#[test]
fn first_send_returns_authoritative_user_message_while_session_resume_is_blocked() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/workspace/app");
    draft.first_prompt_sent = false;
    draft.agent_session_id = Some("prepared-session".to_string());
    store.write_task(&draft).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_resume: AtomicBool::new(true),
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
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = api.send(send_params("task-draft", 1, "first-send", "hello"));
        finished_tx.send(result).unwrap();
    });

    let accepted = finished_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("task/send must not wait for ACP session resume")
        .unwrap();
    assert!(accepted.task.chat.items.iter().any(|item| {
        item.role == openaide_app_server_protocol::snapshot::ChatRole::User
            && matches!(item.parts.first(), Some(MessagePart::Text { text }) if text == "hello")
    }));
    agent.block_resume.store(false, Ordering::SeqCst);
}

#[test]
fn first_send_replaces_a_prepared_session_missing_from_the_agent_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/workspace/app");
    draft.first_prompt_sent = false;
    draft.agent_session_id = Some("missing-session".to_string());
    store.write_task(&draft).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_session_missing: true,
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

    api.send(send_params("task-draft", 1, "first-send", "hello"))
        .unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        store.read_task("task-draft").unwrap().agent_session_id,
        Some("recorded-session".to_string())
    );
}

#[test]
fn send_projects_agent_config_catalog_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        config_catalog: Some(config_catalog("gpt-5")),
        ..Default::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();
    let task_id = "task-existing";

    wait_until(|| {
        store
            .read_task(task_id)
            .ok()
            .and_then(|task| task.config_options_catalog)
            .is_some()
    });

    let opened = project_stored_task_snapshot(
        crate::tasks::snapshot::build_snapshot(&store, task_id, 100).unwrap(),
    )
    .unwrap();

    assert_eq!(opened.agent_config.options.len(), 1);
    let option = &opened.agent_config.options[0];
    assert_eq!(option.config_id.as_str(), "model");
    assert_eq!(option.label, "Model");
    assert_eq!(option.description.as_deref(), Some("Select model"));
    assert_eq!(option.category.as_deref(), Some("model"));
    assert_eq!(option.current_value, "gpt-5");
    assert_eq!(option.values.len(), 2);
    assert_eq!(option.values[1].value, "gpt-5.5");
    assert_eq!(option.values[1].label, "GPT 5.5");
}

#[test]
fn send_projects_agent_command_catalog_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        commands_catalog: Some(command_catalog()),
        ..Default::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();
    let task_id = "task-existing";

    wait_until(|| {
        store
            .read_task(task_id)
            .ok()
            .and_then(|task| task.agent_commands_catalog)
            .is_some()
    });

    let opened = project_stored_task_snapshot(
        crate::tasks::snapshot::build_snapshot(&store, task_id, 100).unwrap(),
    )
    .unwrap();

    assert_eq!(opened.agent_commands.state, LiveSessionDataState::Ready);
    assert_eq!(opened.agent_commands.commands.len(), 1);
    let command = &opened.agent_commands.commands[0];
    assert_eq!(command.name, "web");
    assert_eq!(command.description, "Search the web");
    assert_eq!(
        command.input.as_ref().map(|input| input.hint.as_str()),
        Some("query")
    );
}

#[test]
fn startup_marks_abandoned_preparation_failed_instead_of_loading_forever() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-preparing", "/workspace/app");
    task.first_prompt_sent = false;
    task.preparation = TaskPreparationRecord::Preparing;
    store.write_task(&task).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let record = store.read_task("task-preparing").unwrap();
    assert!(matches!(
        record.preparation,
        TaskPreparationRecord::Failed { .. }
    ));
    let accepted = api
        .send(send_params(
            "task-preparing",
            record.revision,
            "send-1",
            "hello",
        ))
        .unwrap();
    wait_until(|| {
        store
            .read_task("task-preparing")
            .map(|task| task.status == TaskStatus::Failed)
            .unwrap_or(false)
    });
    assert!(accepted.turn_id.as_str().starts_with("turn_"));
}

#[test]
fn create_rejects_unknown_project() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .create(TaskCreateParams {
            project_id: ProjectId::from("project-missing"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
}

#[test]
fn create_accepts_new_workspace_root_for_unknown_project() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let workspace_root = "/workspace/new-app";

    let snapshot = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace(workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: Some(workspace_root.to_string()),
            config_options: Default::default(),
        })
        .unwrap();

    let record = store.read_task(snapshot.task.task_id.as_str()).unwrap();
    assert_eq!(record.workspace_root, workspace_root);
    assert_eq!(
        snapshot.task.project_id,
        project_id_for_workspace(workspace_root)
    );
}

#[test]
fn create_rejects_mismatched_new_workspace_root_project_id() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace("/workspace/other"),
            agent_id: AgentId::from("codex"),
            workspace_root: Some("/workspace/new-app".to_string()),
            config_options: Default::default(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
}

#[test]
fn workspace_directory_lists_child_directories_for_picker() {
    let temp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let store = Store::open(state.path().to_path_buf()).unwrap();
    let workspace_parent = temp.path().join("workspaces");
    std::fs::create_dir(&workspace_parent).unwrap();
    std::fs::create_dir(workspace_parent.join("app")).unwrap();
    std::fs::write(workspace_parent.join("README.md"), "readme").unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let result = api
        .workspace_directory(WorkspaceListDirectoryParams {
            path: workspace_parent.to_string_lossy().to_string(),
        })
        .unwrap();

    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].label, "app");
    assert!(result.entries[0].path.ends_with("/app"));
}

#[test]
fn create_rejects_unknown_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace("/workspace/app"),
            agent_id: AgentId::from("missing-agent"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::CapabilityUnavailable);
}

#[test]
fn list_agent_sessions_filters_already_adopted_native_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.agent_session_id = Some("mock-session".to_string());
    store.write_task(&task).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let result = api
        .list_agent_sessions(AgentListSessionsParams {
            agent_id: AgentId::from("codex"),
            project_id: project_id_for_workspace("/workspace/app"),
            cursor: None,
        })
        .unwrap();

    assert!(result.sessions.is_empty());
}

#[test]
fn list_agent_sessions_hides_a_native_session_while_draft_ownership_is_committing() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_attach: AtomicBool::new(true),
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "recorded-session".to_string(),
            cwd: "/workspace/app".to_string(),
            title: Some("New task".to_string()),
            updated_at: None,
            last_activity: None,
        }]),
        ..RecordingAgent::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.create(TaskCreateParams {
        project_id: project_id_for_workspace("/workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
        config_options: Default::default(),
    })
    .unwrap();
    wait_until(|| agent.attaches.load(Ordering::SeqCst) == 1);

    let result = api
        .list_agent_sessions(AgentListSessionsParams {
            agent_id: AgentId::from("codex"),
            project_id: project_id_for_workspace("/workspace/app"),
            cursor: None,
        })
        .unwrap();
    agent.block_attach.store(false, Ordering::SeqCst);

    assert!(result.sessions.is_empty());
}

#[test]
fn list_agent_sessions_skips_filtered_empty_pages() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(PagedSessionAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let result = api
        .list_agent_sessions(AgentListSessionsParams {
            agent_id: AgentId::from("codex"),
            project_id: project_id_for_workspace("/workspace/app"),
            cursor: None,
        })
        .unwrap();

    assert_eq!(
        agent.requested_cursors(),
        vec![None, Some("page-2".to_string())]
    );
    assert_eq!(
        result
            .sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["matching-session"]
    );
    assert_eq!(result.next_cursor.as_deref(), Some("page-3"));
}

#[test]
fn open_readopts_adopted_task_when_native_session_is_newer_than_cached_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.updated_at = "2026-01-01T00:00:00.000Z".to_string();
    task.last_activity = "2026-01-01T00:00:00.000Z".to_string();
    store.write_task(&task).unwrap();
    store
        .append_message(
            "task-existing",
            ChatMessage {
                cursor: "m:1".to_string(),
                identity: "cached:stale".to_string(),
                message_type: "agent_text".to_string(),
                message_id: "cached_message".to_string(),
                message: NormalizedMessage::AgentText {
                    id: "cached:stale".to_string(),
                    text: "Stale cached history.".to_string(),
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                    streaming: false,
                },
            },
        )
        .unwrap();
    let mut task = store.read_task("task-existing").unwrap();
    task.message_history_version = store.message_history_version("task-existing").unwrap();
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/workspace/app".to_string(),
            title: Some("Native title".to_string()),
            last_activity: Some("2026-01-02T00:00:00.000Z".to_string()),
            updated_at: Some("2026-01-02T00:00:00.000Z".to_string()),
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::AgentText {
            id: "native:fresh".to_string(),
            text: "Fresh native history.".to_string(),
            created_at: "2026-01-02T00:00:00.000Z".to_string(),
            streaming: false,
        }]),
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

    let snapshot = api
        .open(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(snapshot.task.title, "Native title");
    assert_eq!(snapshot.task.updated_at, "2026-01-02T00:00:00.000Z");
    assert_eq!(snapshot.chat.items.len(), 1);
    assert!(matches!(
        snapshot.chat.items[0].parts.first(),
        Some(MessagePart::Text { text }) if text == "Fresh native history."
    ));
    let stored_messages = store.read_messages("task-existing").unwrap();
    assert_eq!(stored_messages.len(), 1);
    assert!(matches!(
        &stored_messages[0].chat.message,
        NormalizedMessage::AgentText { text, .. } if text == "Fresh native history."
    ));
    let record = store.read_task("task-existing").unwrap();
    assert!(!record.unread);
    assert_eq!(record.last_activity, "2026-01-02T00:00:00.000Z");
}

#[test]
fn open_does_not_load_native_history_for_an_unsent_draft() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-draft", "/workspace/app");
    task.first_prompt_sent = false;
    task.agent_session_id = Some("prepared-session".to_string());
    task.updated_at = "2026-01-01T00:00:00.000Z".to_string();
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "prepared-session".to_string(),
            cwd: "/workspace/app".to_string(),
            title: None,
            last_activity: Some("2026-01-02T00:00:00.000Z".to_string()),
            updated_at: Some("2026-01-02T00:00:00.000Z".to_string()),
        }]),
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

    api.open(TaskOpenParams {
        task_id: "task-draft".into(),
    })
    .unwrap();

    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
}

#[test]
fn open_readopt_retries_when_adopted_session_is_already_active() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.updated_at = "2026-01-01T00:00:00.000Z".to_string();
    task.last_activity = "2026-01-01T00:00:00.000Z".to_string();
    store.write_task(&task).unwrap();
    store
        .append_message(
            "task-existing",
            ChatMessage {
                cursor: "m:1".to_string(),
                identity: "cached:stale".to_string(),
                message_type: "agent_text".to_string(),
                message_id: "cached_message".to_string(),
                message: NormalizedMessage::AgentText {
                    id: "cached:stale".to_string(),
                    text: "Stale cached history.".to_string(),
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                    streaming: false,
                },
            },
        )
        .unwrap();
    let mut task = store.read_task("task-existing").unwrap();
    task.message_history_version = store.message_history_version("task-existing").unwrap();
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/workspace/app".to_string(),
            title: Some("Native title".to_string()),
            last_activity: Some("2026-01-02T00:00:00.000Z".to_string()),
            updated_at: Some("2026-01-02T00:00:00.000Z".to_string()),
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::AgentText {
            id: "native:fresh".to_string(),
            text: "Fresh native history.".to_string(),
            created_at: "2026-01-02T00:00:00.000Z".to_string(),
            streaming: false,
        }]),
        fail_load_once_with_already_active: AtomicBool::new(true),
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

    let snapshot = api
        .open(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert_eq!(agent.loads.load(Ordering::SeqCst), 2);
    assert_eq!(agent.closes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(snapshot.chat.items.len(), 1);
    assert!(matches!(
        snapshot.chat.items[0].parts.first(),
        Some(MessagePart::Text { text }) if text == "Fresh native history."
    ));
}

#[test]
fn open_keeps_adopted_task_cache_when_native_session_is_not_newer() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.unread = true;
    task.updated_at = "2026-01-02T00:00:00.000Z".to_string();
    task.last_activity = "2026-01-02T00:00:00.000Z".to_string();
    store.write_task(&task).unwrap();
    store
        .append_message(
            "task-existing",
            ChatMessage {
                cursor: "m:1".to_string(),
                identity: "cached:current".to_string(),
                message_type: "agent_text".to_string(),
                message_id: "cached_message".to_string(),
                message: NormalizedMessage::AgentText {
                    id: "cached:current".to_string(),
                    text: "Current cached history.".to_string(),
                    created_at: "2026-01-02T00:00:00.000Z".to_string(),
                    streaming: false,
                },
            },
        )
        .unwrap();
    let mut task = store.read_task("task-existing").unwrap();
    task.message_history_version = store.message_history_version("task-existing").unwrap();
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/workspace/app".to_string(),
            title: Some("Older native title".to_string()),
            last_activity: Some("2026-01-01T00:00:00.000Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00.000Z".to_string()),
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::AgentText {
            id: "native:older".to_string(),
            text: "Older native history.".to_string(),
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            streaming: false,
        }]),
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

    let snapshot = api
        .open(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 0);
    assert_eq!(snapshot.task.title, "Existing");
    assert!(matches!(
        snapshot.chat.items[0].parts.first(),
        Some(MessagePart::Text { text }) if text == "Current cached history."
    ));
    let record = store.read_task("task-existing").unwrap();
    assert!(!record.unread);
    assert_eq!(record.last_activity, "2026-01-02T00:00:00.000Z");
}

#[test]
fn mark_read_clears_unread_without_refreshing_native_session_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.unread = true;
    task.agent_session_id = Some("native-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = api
        .mark_read(TaskMarkReadParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert!(!snapshot.task.unread);
    assert!(!store.read_task("task-existing").unwrap().unread);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
}

#[test]
fn send_commits_user_message_and_running_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let accepted = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    let record = store.read_task("task-existing").unwrap();
    assert!(record.first_prompt_sent);
    if record.status == TaskStatus::Active {
        assert_eq!(
            record.active_turn_id.as_deref(),
            Some(accepted.turn_id.as_str())
        );
    } else {
        assert_eq!(record.status, TaskStatus::Inactive);
        assert_eq!(record.active_turn_id, None);
    }
    assert!(record.message_history_version >= 2);
    assert!(accepted.task.chat.items.len() >= 2);
    assert_eq!(
        accepted.task.chat.items[0].message_id,
        accepted.user_message_id
    );
}

#[test]
fn send_returns_after_durable_acceptance_without_waiting_for_session_start() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    agent.block_start.store(true, Ordering::SeqCst);
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        accepted_tx
            .send(api.send(send_params(
                "task-existing",
                1,
                "send-fast-acceptance",
                "hello",
            )))
            .unwrap();
    });

    wait_until(|| agent.starts.load(Ordering::SeqCst) == 1);
    let accepted = accepted_rx.recv_timeout(Duration::from_millis(100));
    agent.block_start.store(false, Ordering::SeqCst);

    assert!(accepted.is_ok(), "Send waited for Native Session startup");
    assert!(accepted.unwrap().unwrap().task.chat.items.len() >= 2);
}

#[test]
fn first_send_is_accepted_while_draft_session_preparation_is_running() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut project_anchor = task_record("task-project-anchor", "/workspace/app");
    project_anchor.first_prompt_sent = true;
    store.write_task(&project_anchor).unwrap();
    let agent = Arc::new(RecordingAgent::default());
    agent.block_start.store(true, Ordering::SeqCst);
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let draft = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace("/workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
            config_options: Default::default(),
        })
        .unwrap();
    wait_until(|| agent.starts.load(Ordering::SeqCst) == 1);

    let accepted = api.send(send_params(
        draft.task.task_id.as_str(),
        draft.revision,
        "send-during-preparation",
        "hello",
    ));
    agent.block_start.store(false, Ordering::SeqCst);

    assert!(accepted.is_ok(), "first Send waited for Task preparation");
}

#[test]
fn send_starts_agent_session_and_prompts_after_commit() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(
        store.read_task("task-existing").unwrap().agent_session_id,
        Some("recorded-session".to_string())
    );
}

#[test]
fn send_recovers_stale_active_turn_and_starts_current_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let mut stale = store.read_task("task-existing").unwrap();
    stale.status = TaskStatus::Active;
    stale.active_turn_id = Some("turn-stale".to_string());
    store.write_task(&stale).unwrap();
    append_running_turn(&store, "task-existing", "turn-stale");
    let stale_revision = store.read_task("task-existing").unwrap().revision;

    let accepted = api
        .send(send_params(
            "task-existing",
            stale_revision,
            "send-1",
            "why stuck",
        ))
        .unwrap();

    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    let record = store.read_task("task-existing").unwrap();
    assert_eq!(record.status, TaskStatus::Inactive);
    assert_eq!(record.active_turn_id, None);
    assert_ne!(accepted.turn_id.as_str(), "turn-stale");
    assert_eq!(
        agent.prompt_calls.lock().unwrap().as_slice(),
        &[("recorded-session".to_string(), "why stuck".to_string())]
    );

    let messages = store.read_messages("task-existing").unwrap();
    assert!(messages.iter().any(|message| {
        matches!(
            message.chat.message,
            NormalizedMessage::Activity {
                ref id,
                status: ActivityStatus::Completed,
                ..
            } if id == "turn:turn-stale"
        )
    }));
    assert!(messages.iter().any(|message| {
        matches!(
            message.chat.message,
            NormalizedMessage::Interruption {
                reason: InterruptionReason::Canceled,
                recoverable: true,
                ..
            }
        )
    }));
    assert!(messages.iter().any(|message| {
        matches!(
            message.chat.message,
            NormalizedMessage::User { ref text, .. } if text == "why stuck"
        )
    }));
}

#[test]
fn send_loads_stored_agent_session_when_live_resume_is_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.agent_session_id = Some("stored-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        loaded_session_id: Some("stored-session".to_string()),
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

    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.read_task("task-existing").unwrap().agent_session_id,
        Some("stored-session".to_string())
    );
}

#[test]
fn send_after_restart_starts_fresh_session_when_stored_session_load_times_out() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.agent_session_id = Some("stored-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        load_start_timeout: true,
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

    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        agent.prompt_calls.lock().unwrap().as_slice(),
        &[("recorded-session".to_string(), "hello".to_string())]
    );
    assert_eq!(
        store.read_task("task-existing").unwrap().agent_session_id,
        Some("recorded-session".to_string())
    );
}

#[test]
fn send_rejects_task_when_current_agent_registry_no_longer_has_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.agent_session_id = Some("recorded-session".to_string());
    store.write_task(&task).unwrap();
    let registry = AgentRegistryHandle::new(AgentRegistry::default_built_ins());
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        registry.clone(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    registry.replace(
        AgentRegistry::from_catalog_overlay(vec![AgentCatalogRecord::disabled_builtin(
            "codex".to_string(),
        )])
        .unwrap(),
    );

    let error = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::CapabilityUnavailable);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    assert!(store.read_messages("task-existing").unwrap().is_empty());
}

#[test]
fn send_tolerates_attach_time_command_catalog_revision_bump() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        commands_catalog: Some(command_catalog()),
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

    let accepted = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    let record = store.read_task("task-existing").unwrap();
    assert_eq!(
        accepted.user_message_id.as_str(),
        accepted.task.chat.items[0].message_id.as_str()
    );
    assert!(store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .any(|message| matches!(message.chat.message, NormalizedMessage::User { .. })));
    assert_eq!(
        record
            .agent_commands_catalog
            .as_ref()
            .and_then(|catalog| catalog.commands.first())
            .map(|command| command.name.as_str()),
        Some("web")
    );
}

#[test]
fn send_start_failure_returns_accepted_failed_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        fail_start: true,
        ..RecordingAgent::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let params = send_params("task-existing", 1, "send-1", "hello");
    let accepted = api
        .send(params.clone())
        .expect("a durably committed send must remain accepted");
    let retry = api.send(params).unwrap();

    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.status == TaskStatus::Failed)
            .unwrap_or(false)
    });

    assert_eq!(retry.turn_id, accepted.turn_id);
    assert_eq!(retry.user_message_id, accepted.user_message_id);
    let messages = store.read_messages("task-existing").unwrap();
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::User { ref text, .. } if text == "hello"
    )));
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Error,
            ..
        }
    )));
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::Interruption {
            reason: InterruptionReason::Failed,
            recoverable: true,
            ..
        }
    )));
    let task = store.read_task("task-existing").unwrap();
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(task.agent_session_id, None);
}

#[test]
fn send_session_attach_failure_returns_accepted_failed_turn_and_closes_new_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        fail_attach: true,
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
    let params = send_params("task-existing", 1, "send-1", "hello");

    let accepted = api
        .send(params.clone())
        .expect("a durably committed send must remain accepted");
    let retry = api.send(params).unwrap();

    wait_until(|| agent.closes.load(Ordering::SeqCst) == 1);

    assert_eq!(retry.turn_id, accepted.turn_id);
    assert_eq!(retry.user_message_id, accepted.user_message_id);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(agent.closes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    let task = store.read_task("task-existing").unwrap();
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(task.agent_session_id, None);
}

#[test]
fn send_snapshot_failure_after_commit_preserves_idempotent_accepted_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let params = send_params("task-existing", 1, "send-1", "hello");
    store.fail_next_tail_page_for_test();

    let error = api.send(params.clone()).unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::Internal);
    let retry = api.send(params).unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert!(retry.turn_id.as_str().starts_with("turn_"));
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 1);
    assert_eq!(
        store
            .read_messages("task-existing")
            .unwrap()
            .iter()
            .filter(|message| matches!(message.chat.message, NormalizedMessage::User { .. }))
            .count(),
        1
    );
}

#[test]
fn send_retry_after_process_crash_before_task_record_does_not_duplicate() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.title = "New task".to_string();
    record.first_prompt_sent = false;
    store.write_task(&record).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let params = send_params("task-existing", 1, "send-crash", "hello");
    store.crash_before_next_task_write_for_test();

    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = api.send(params.clone());
    }));

    assert!(crashed.is_err());
    let committed_receipt = store
        .read_send_receipt("task-existing", "send-crash")
        .unwrap()
        .expect("receipt must identify the accepted send before Task persistence");
    assert_eq!(
        store
            .read_messages("task-existing")
            .unwrap()
            .iter()
            .filter(|message| matches!(message.chat.message, NormalizedMessage::User { .. }))
            .count(),
        1
    );
    drop(api);
    drop(store);

    let reopened_store = Store::open(temp.path().to_path_buf()).unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let reopened_api = TaskProductApi::new(
        reopened_store.clone(),
        Arc::new(StorageProjectResolver::new(reopened_store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let retried = reopened_api.send(params).unwrap();

    assert_eq!(retried.turn_id.as_str(), committed_receipt.turn_id);
    assert_eq!(
        retried.user_message_id.as_str(),
        committed_receipt.user_message_id
    );
    assert_eq!(agent.starts.load(Ordering::SeqCst), 0);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    let messages = reopened_store.read_messages("task-existing").unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(message.chat.message, NormalizedMessage::User { .. }))
            .count(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| {
                message.chat.identity == format!("turn:{}", retried.turn_id.as_str())
            })
            .count(),
        1
    );
    assert!(messages.iter().any(|message| matches!(
        &message.chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Completed,
            ..
        } if message.chat.identity == format!("turn:{}", retried.turn_id.as_str())
    )));
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                &message.chat.message,
                NormalizedMessage::Interruption { message, .. }
                    if message == crate::task_recovery::RESTART_INTERRUPTION_MESSAGE
            ))
            .count(),
        1
    );
    let task = reopened_store.read_task("task-existing").unwrap();
    assert_eq!(task.status, TaskStatus::Inactive);
    assert_eq!(task.active_turn_id, None);
    assert!(task.first_prompt_sent);
    assert_eq!(task.title, "hello");
    assert_eq!(
        task.message_history_version,
        reopened_store
            .message_history_version("task-existing")
            .unwrap()
    );
}

#[test]
fn send_ignores_orphan_receipt_without_durable_user_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    store
        .write_send_receipt(
            "task-existing",
            TaskSendReceipt {
                idempotency_key: "send-1".to_string(),
                text: "hello".to_string(),
                attachment_handles: Vec::new(),
                user_message_id: "orphan-message".to_string(),
                turn_id: "orphan-turn".to_string(),
            },
        )
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let accepted = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_ne!(accepted.user_message_id.as_str(), "orphan-message");
    assert_ne!(accepted.turn_id.as_str(), "orphan-turn");
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 1);
    assert_eq!(
        store
            .read_messages("task-existing")
            .unwrap()
            .iter()
            .filter(|message| matches!(message.chat.message, NormalizedMessage::User { .. }))
            .count(),
        1
    );
}

#[test]
fn send_post_commit_start_failure_consumes_attachment_and_returns_accepted_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let attachment_path = workspace.join("notes.md");
    std::fs::write(&attachment_path, "hello").unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            workspace.to_string_lossy().as_ref(),
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        fail_start: true,
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
    let handle = api.attachment_runtime().register_file_reference_for_test(
        TaskId::from("task-existing"),
        "notes.md",
        attachment_path,
    );
    let handle_id = handle.handle_id.clone();

    let params = TaskSendParams {
        task_id: "task-existing".into(),
        idempotency_key: "send-1".into(),
        task_revision: 1,
        message: ComposerMessage {
            text: Some("hello".to_string()),
            attachments: vec![handle_id.clone()],
        },
    };
    let accepted = api
        .send(params.clone())
        .expect("a durably committed send must remain accepted");
    let retry = api.send(params).unwrap();
    let reuse_error = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            idempotency_key: "send-2".into(),
            task_revision: 1,
            message: ComposerMessage {
                text: Some("reuse".to_string()),
                attachments: vec![handle_id],
            },
        })
        .unwrap_err();

    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.status == TaskStatus::Failed)
            .unwrap_or(false)
    });

    assert_eq!(retry.turn_id, accepted.turn_id);
    assert_eq!(retry.user_message_id, accepted.user_message_id);
    assert_eq!(reuse_error.code, ProtocolErrorCode::AttachmentHandleInvalid);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    let messages = store.read_messages("task-existing").unwrap();
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::User { ref text, .. } if text == "hello"
    )));
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Error,
            ..
        }
    )));
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::Interruption {
            reason: InterruptionReason::Failed,
            recoverable: true,
            ..
        }
    )));
    let task = store.read_task("task-existing").unwrap();
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(task.agent_session_id, None);
}

#[test]
fn send_reservation_wins_release_race_after_durable_commit() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let attachment_path = workspace.join("notes.md");
    std::fs::write(&attachment_path, "hello").unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            workspace.to_string_lossy().as_ref(),
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let (notifier, commit_blocker) = TaskUpdateNotifier::blocking_once_for_test();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        notifier,
    )
    .unwrap();
    let attachments = api.attachment_runtime();
    let task_id = TaskId::from("task-existing");
    let handle =
        attachments.register_file_reference_for_test(task_id.clone(), "notes.md", attachment_path);
    let handle_id = handle.handle_id.clone();
    let send_api = api.clone();
    let send_thread = std::thread::spawn(move || {
        send_api.send(TaskSendParams {
            task_id: "task-existing".into(),
            idempotency_key: "send-1".into(),
            task_revision: 1,
            message: ComposerMessage {
                text: Some("hello".to_string()),
                attachments: vec![handle_id],
            },
        })
    });

    commit_blocker.wait_until_blocked();
    let released = attachments.release_handles(&task_id, &[handle.handle_id]);
    commit_blocker.release();
    let accepted = send_thread.join().unwrap().unwrap();

    assert!(released.released_handles.is_empty());
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 1);
}

#[test]
fn send_start_failure_does_not_poison_later_task_start() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-first", "/workspace/app"))
        .unwrap();
    store
        .write_task(&task_record("task-second", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        fail_start_once: AtomicBool::new(true),
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

    let accepted = api
        .send(send_params("task-first", 1, "send-1", "first"))
        .expect("a durably committed send must remain accepted");

    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    wait_until(|| {
        store
            .read_task("task-first")
            .map(|task| task.status == TaskStatus::Failed)
            .unwrap_or(false)
    });
    let first = store.read_task("task-first").unwrap();
    assert_eq!(first.status, TaskStatus::Failed);
    assert_eq!(first.active_turn_id, None);
    assert_eq!(first.agent_session_id, None);

    api.send(send_params("task-second", 1, "send-2", "second"))
        .unwrap();
    wait_until(|| {
        agent.prompts.load(Ordering::SeqCst) == 1
            && store
                .read_task("task-second")
                .map(|task| task.status == TaskStatus::Inactive)
                .unwrap_or(false)
    });

    let second = store.read_task("task-second").unwrap();
    assert_eq!(second.status, TaskStatus::Inactive);
    assert_eq!(second.active_turn_id, None);
    assert_eq!(second.agent_session_id.as_deref(), Some("recorded-session"));
    assert_eq!(agent.starts.load(Ordering::SeqCst), 2);
}

#[test]
fn send_retries_same_idempotency_key_without_duplicate_messages() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let first = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();
    let retry = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    assert_eq!(retry.turn_id, first.turn_id);
    assert_eq!(retry.user_message_id, first.user_message_id);
    assert_eq!(
        store
            .read_messages("task-existing")
            .unwrap()
            .iter()
            .filter(|message| matches!(message.chat.message, NormalizedMessage::User { .. }))
            .count(),
        1
    );
}

#[test]
fn send_retry_does_not_prompt_agent_again() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();
    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 1);
}

#[test]
fn send_retry_returns_the_turn_created_for_that_idempotency_key() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    append_old_completed_turn(&store, "task-existing");
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let first = api
        .send(send_params("task-existing", 1, "send-new", "new prompt"))
        .unwrap();
    let retry = api
        .send(send_params("task-existing", 1, "send-new", "new prompt"))
        .unwrap();

    assert_eq!(retry.turn_id, first.turn_id);
    assert_ne!(retry.turn_id.as_str(), "turn_old");
    assert_eq!(retry.user_message_id, first.user_message_id);
}

#[test]
fn send_preserves_non_empty_prompt_text_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let accepted = api
        .send(send_params("task-existing", 1, "send-1", "  indented\n  "))
        .unwrap();

    assert_eq!(
        accepted.task.chat.items[0].parts[0],
        openaide_app_server_protocol::snapshot::MessagePart::Text {
            text: "  indented\n  ".to_string()
        }
    );
}

#[test]
fn send_steers_live_active_turn_immediately() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_prompt: true,
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
    let first = api
        .send(send_params("task-existing", 1, "send-1", "start work"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let active_revision = store.read_task("task-existing").unwrap().revision;
    let steer = api
        .send(send_params(
            "task-existing",
            active_revision,
            "send-steer-1",
            "steer now",
        ))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 2);

    let record = store.read_task("task-existing").unwrap();
    assert_eq!(record.status, TaskStatus::Active);
    assert_eq!(
        record.active_turn_id.as_deref(),
        Some(first.turn_id.as_str())
    );
    assert_eq!(steer.turn_id, first.turn_id);
    assert_eq!(
        agent.prompt_calls.lock().unwrap().clone(),
        vec![
            ("recorded-session".to_string(), "start work".to_string()),
            ("recorded-session".to_string(), "steer now".to_string()),
        ]
    );
    assert!(store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .any(|message| matches!(
            message.chat.message,
            NormalizedMessage::User { ref text, .. } if text == "steer now"
        )));

    api.cancel(TaskCancelParams {
        task_id: "task-existing".into(),
        turn_id: Some(first.turn_id),
    })
    .unwrap();
}

#[test]
fn concurrent_steering_prompts_finish_only_after_the_last_accepted_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_first_prompt: true,
        complete_first_prompt_after_steering: 2,
        block_steering_prompts: true,
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
    let first = api
        .send(send_params("task-existing", 1, "send-1", "start work"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    api.send(send_params(
        "task-existing",
        store.read_task("task-existing").unwrap().revision,
        "send-steer-1",
        "first steering",
    ))
    .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 2);
    api.send(send_params(
        "task-existing",
        store.read_task("task-existing").unwrap().revision,
        "send-steer-2",
        "second steering",
    ))
    .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 3);

    agent.released_steering_prompts.store(1, Ordering::SeqCst);
    wait_until(|| agent.completed_prompts.load(Ordering::SeqCst) >= 1);
    let after_first_steering = store.read_task("task-existing").unwrap();
    assert_eq!(after_first_steering.status, TaskStatus::Active);
    assert_eq!(
        after_first_steering.active_turn_id.as_deref(),
        Some(first.turn_id.as_str())
    );
    assert_eq!(agent.cancels.load(Ordering::SeqCst), 0);

    agent.released_steering_prompts.store(2, Ordering::SeqCst);
    wait_until(|| {
        let task = store.read_task("task-existing").unwrap();
        task.status == TaskStatus::Inactive && task.active_turn_id.is_none()
    });
    assert_eq!(agent.cancels.load(Ordering::SeqCst), 0);
    assert_eq!(
        agent
            .prompt_calls
            .lock()
            .unwrap()
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        ["start work", "first steering", "second steering"]
    );
}

#[test]
fn steering_runner_loss_after_commit_returns_accepted_failed_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_first_prompt: true,
        ..RecordingAgent::default()
    });
    let (notifier, commit_blocker) = TaskUpdateNotifier::blocking_once_for_test();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        notifier,
    )
    .unwrap();
    let first_api = api.clone();
    let first_send = std::thread::spawn(move || {
        first_api.send(send_params("task-existing", 1, "send-1", "start work"))
    });
    commit_blocker.wait_until_blocked();
    commit_blocker.release();
    let first = first_send.join().unwrap().unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let params = send_params(
        "task-existing",
        store.read_task("task-existing").unwrap().revision,
        "send-steer-1",
        "steer now",
    );
    commit_blocker.rearm();
    let steer_api = api.clone();
    let steer_params = params.clone();
    let steering_send = std::thread::spawn(move || steer_api.send(steer_params));
    commit_blocker.wait_until_blocked();
    api.turn_runner.detach_stuck_turn(first.turn_id.as_str());
    commit_blocker.release();
    let accepted = steering_send.join().unwrap().unwrap();
    let retry = api.send(params).unwrap();

    assert_eq!(retry.turn_id, accepted.turn_id);
    assert_eq!(retry.user_message_id, accepted.user_message_id);
    assert_eq!(accepted.turn_id, first.turn_id);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 1);
    let task = store.read_task("task-existing").unwrap();
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.active_turn_id, None);
    assert!(store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .any(|message| matches!(
            message.chat.message,
            NormalizedMessage::User { ref text, .. } if text == "steer now"
        )));
}

#[test]
fn send_rejects_steering_while_active_turn_is_blocked_on_permission() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_prompt: true,
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
    let first = api
        .send(send_params("task-existing", 1, "send-1", "start work"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let mut blocked = store.read_task("task-existing").unwrap();
    blocked.status = TaskStatus::Blocked;
    store.write_task(&blocked).unwrap();
    let blocked_revision = store.read_task("task-existing").unwrap().revision;

    let error = api
        .send(send_params(
            "task-existing",
            blocked_revision,
            "send-steer-1",
            "why no answer?",
        ))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 1);
    let record = store.read_task("task-existing").unwrap();
    assert_eq!(record.status, TaskStatus::Blocked);
    assert_eq!(
        record.active_turn_id.as_deref(),
        Some(first.turn_id.as_str())
    );
    assert!(!store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .any(|message| matches!(
            message.chat.message,
            NormalizedMessage::User { ref text, .. } if text == "why no answer?"
        )));

    api.cancel(TaskCancelParams {
        task_id: "task-existing".into(),
        turn_id: Some(first.turn_id),
    })
    .unwrap();
}

#[test]
fn coalesced_steering_completion_finishes_active_turn_without_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_first_prompt: true,
        complete_first_prompt_after_steering: 1,
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
    api.send(send_params("task-existing", 1, "send-1", "start work"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let active_revision = store.read_task("task-existing").unwrap().revision;
    api.send(send_params(
        "task-existing",
        active_revision,
        "send-steer-1",
        "stop now",
    ))
    .unwrap();

    wait_until(|| {
        let record = store.read_task("task-existing").unwrap();
        agent.prompts.load(Ordering::SeqCst) == 2
            && agent.cancels.load(Ordering::SeqCst) == 0
            && record.status == TaskStatus::Inactive
            && record.active_turn_id.is_none()
    });
    assert_eq!(
        agent.prompt_calls.lock().unwrap().clone(),
        vec![
            ("recorded-session".to_string(), "start work".to_string()),
            ("recorded-session".to_string(), "stop now".to_string()),
        ]
    );
    assert_eq!(
        store.read_task("task-existing").unwrap().agent_session_id,
        Some("recorded-session".to_string())
    );
}

#[test]
fn send_rejects_same_idempotency_key_with_different_message() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    let error = api
        .send(send_params("task-existing", 1, "send-1", "changed"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
}

#[test]
fn send_rejects_stale_revision_for_new_submission() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .send(send_params("task-existing", 0, "send-1", "hello"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert!(store.read_messages("task-existing").unwrap().is_empty());
}

#[test]
fn send_keeps_committed_message_when_config_changes_while_agent_session_opens() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(ConfigMutatingStartAgent {
        store: store.clone(),
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let accepted = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.config_options.get("model") == Some(&"new-model".to_string()))
            .unwrap_or(false)
    });

    let record = store.read_task("task-existing").unwrap();
    assert_eq!(
        record.config_options.get("model"),
        Some(&"new-model".to_string())
    );
    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    assert!(store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .any(|message| matches!(
            message.chat.message,
            NormalizedMessage::User { ref text, .. } if text == "hello"
        )));
}

#[test]
fn send_rejects_unknown_attachment_handles_with_reselection_error() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            idempotency_key: "send-1".into(),
            task_revision: 1,
            message: ComposerMessage {
                text: Some("hello".to_string()),
                attachments: vec!["attachment-1".into()],
            },
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::AttachmentHandleInvalid);
    assert_eq!(
        error.message,
        "Attachment is no longer available. Reselect it and try again."
    );
    assert!(error.recoverable);
    assert!(store.read_messages("task-existing").unwrap().is_empty());
    assert_eq!(
        store.read_task("task-existing").unwrap().active_turn_id,
        None
    );
}

#[test]
fn send_commits_valid_attachment_handles_as_safe_chat_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let attachment_path = workspace.join("notes.md");
    std::fs::write(&attachment_path, "hello").unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            workspace.to_string_lossy().as_ref(),
        ))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let handle = api.attachment_runtime().register_file_reference_for_test(
        TaskId::from("task-existing"),
        "notes.md",
        attachment_path,
    );

    let accepted = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            idempotency_key: "send-1".into(),
            task_revision: 1,
            message: ComposerMessage {
                text: Some("hello".to_string()),
                attachments: vec![handle.handle_id.clone()],
            },
        })
        .unwrap();
    let retry = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            idempotency_key: "send-1".into(),
            task_revision: 1,
            message: ComposerMessage {
                text: Some("hello".to_string()),
                attachments: vec![handle.handle_id],
            },
        })
        .unwrap();

    assert_eq!(retry.user_message_id, accepted.user_message_id);
    assert_eq!(accepted.task.chat.items[0].parts.len(), 2);
    let openaide_app_server_protocol::snapshot::MessagePart::Attachment { attachment } =
        &accepted.task.chat.items[0].parts[1]
    else {
        panic!("expected attachment part");
    };
    assert_eq!(attachment.label, "notes.md");
}

#[test]
fn send_commits_attachment_only_image_without_an_empty_text_part() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/workspace/app");
    task.title = "New task".to_string();
    task.first_prompt_sent = false;
    store.write_task(&task).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let image = api
        .attachment_runtime()
        .create_pasted_image(
            TaskId::from("task-existing"),
            "pasted.png",
            "image/png",
            "aW1hZ2U=",
        )
        .unwrap();

    let accepted = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            idempotency_key: "send-image-only".into(),
            task_revision: 1,
            message: ComposerMessage {
                text: None,
                attachments: vec![image.attachment.handle_id],
            },
        })
        .unwrap();

    assert_eq!(accepted.task.chat.items[0].parts.len(), 1);
    let MessagePart::Attachment { attachment } = &accepted.task.chat.items[0].parts[0] else {
        panic!("expected attachment-only user message");
    };
    assert_eq!(attachment.label, "pasted.png");
    assert_eq!(accepted.task.task.title, "Untitled task");
    assert_eq!(
        store.read_task("task-existing").unwrap().title,
        "Untitled task"
    );
}

#[test]
fn rejected_send_releases_attachment_reservation_for_retry() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let attachment_path = workspace.join("notes.md");
    std::fs::write(&attachment_path, "hello").unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            workspace.to_string_lossy().as_ref(),
        ))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let handle = api.attachment_runtime().register_file_reference_for_test(
        TaskId::from("task-existing"),
        "notes.md",
        attachment_path,
    );
    let message = ComposerMessage {
        text: Some("hello".to_string()),
        attachments: vec![handle.handle_id],
    };

    let error = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            idempotency_key: "stale-send".into(),
            task_revision: 0,
            message: message.clone(),
        })
        .unwrap_err();
    let accepted = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            idempotency_key: "current-send".into(),
            task_revision: 1,
            message,
        })
        .unwrap();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert_eq!(accepted.task.chat.items[0].parts.len(), 2);
}

#[cfg(unix)]
#[test]
fn send_rejects_a_selected_file_replaced_with_an_escaping_symlink_without_committing() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let selected = workspace.join("notes.txt");
    let secret = outside.join("secret.txt");
    std::fs::write(&selected, "inside").unwrap();
    std::fs::write(&secret, "outside").unwrap();

    let store = Store::open(temp.path().join("store")).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            workspace.to_string_lossy().as_ref(),
        ))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let task_id = openaide_app_server_protocol::ids::TaskId::from("task-existing");
    let root = api
        .attachment_runtime()
        .list_roots(&task_id, &workspace)
        .roots
        .remove(0);
    let listing = api
        .attachment_runtime()
        .list_directory(&task_id, &workspace, &root.root_id, None)
        .unwrap();
    let handle = api
        .attachment_runtime()
        .create_file_reference(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .attachment
        .handle_id;

    std::fs::remove_file(&selected).unwrap();
    symlink(&secret, &selected).unwrap();

    let error = api
        .send(TaskSendParams {
            task_id,
            idempotency_key: "send-1".into(),
            task_revision: 1,
            message: ComposerMessage {
                text: Some("hello".to_string()),
                attachments: vec![handle],
            },
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::ValidationFailed);
    assert!(!error
        .message
        .contains(temp.path().to_string_lossy().as_ref()));
    assert!(store.read_messages("task-existing").unwrap().is_empty());
}

#[test]
fn cancel_clears_active_turn_and_appends_interruption() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.status = TaskStatus::Active;
    record.active_turn_id = Some("turn-active".to_string());
    store.write_task(&record).unwrap();
    append_running_turn(&store, "task-existing", "turn-active");
    store
        .append_message(
            "task-existing",
            ChatMessage {
                cursor: "m:streaming".to_string(),
                identity: "agent-stream".to_string(),
                message_type: "agent_text".to_string(),
                message_id: "message_streaming".to_string(),
                message: NormalizedMessage::AgentText {
                    id: "agent-stream".to_string(),
                    text: "partial response".to_string(),
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                    streaming: true,
                },
            },
        )
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = api
        .cancel(TaskCancelParams {
            task_id: "task-existing".into(),
            turn_id: Some("turn-active".into()),
        })
        .unwrap();

    let record = store.read_task("task-existing").unwrap();
    assert_eq!(record.status, TaskStatus::Inactive);
    assert_eq!(record.active_turn_id, None);
    assert!(store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .all(|message| !matches!(
            message.chat.message,
            NormalizedMessage::AgentText {
                streaming: true,
                ..
            } | NormalizedMessage::Thought {
                streaming: true,
                ..
            }
        )));
    assert_eq!(
        snapshot.task.status,
        openaide_app_server_protocol::snapshot::TaskStatus::Idle
    );
    assert!(
        store
            .read_messages("task-existing")
            .unwrap()
            .iter()
            .any(|message| matches!(message.chat.message, NormalizedMessage::Interruption { .. })),
        "cancel should append a durable interruption"
    );
}

#[test]
fn cancel_signals_live_agent_turn_started_by_task_send() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_prompt: true,
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
    let sent = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    api.cancel(TaskCancelParams {
        task_id: "task-existing".into(),
        turn_id: Some(sent.turn_id),
    })
    .unwrap();

    wait_until(|| agent.cancels.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.cancels.load(Ordering::SeqCst), 1);
}

#[test]
fn support_recovery_clears_live_stuck_turn_without_waiting_for_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_prompt: true,
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
    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let result = api
        .recover_stuck_sessions(SupportRecoverStuckSessionsParams {})
        .unwrap();

    assert_eq!(result.recovered_tasks.len(), 1);
    wait_until(|| agent.cancels.load(Ordering::SeqCst) == 1);
    let record = store.read_task("task-existing").unwrap();
    assert_eq!(record.status, TaskStatus::Inactive);
    assert_eq!(record.active_turn_id, None);
    assert!(record.unread);
    assert_eq!(
        api.shutdown_blockers().unwrap().active_turns,
        0,
        "support recovery should detach the live turn from runtime blockers"
    );
    assert!(
        store
            .read_messages("task-existing")
            .unwrap()
            .iter()
            .any(|message| matches!(
                &message.chat.message,
                NormalizedMessage::Interruption { message, recoverable: true, .. }
                    if message.contains("support recovery")
            )),
        "support recovery should leave a durable explanation in chat"
    );
}

#[test]
fn cancel_rejects_mismatched_turn_id() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    api.send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap();

    let error = api
        .cancel(TaskCancelParams {
            task_id: "task-existing".into(),
            turn_id: Some("turn-other".into()),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert!(store
        .read_task("task-existing")
        .unwrap()
        .active_turn_id
        .is_some());
}

#[test]
fn set_config_option_persists_for_idle_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = TaskSetConfigOptionWorkflow::set_config_option(
        &api,
        TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: "gpt-5.5".to_string(),
            client_mutation_id: "mutation-1".into(),
        },
    )
    .unwrap();

    assert_eq!(
        store
            .read_task("task-existing")
            .unwrap()
            .config_options
            .get("model"),
        Some(&"gpt-5.5".to_string())
    );
    assert_eq!(
        snapshot.agent_config.pending_change, None,
        "persisted config changes should not leave frontend pending state"
    );
}

#[test]
fn set_config_option_updates_stored_agent_config_catalog_current_value() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    let catalog = config_catalog("gpt-5");
    record.config_options = catalog.current_values();
    record.config_options_catalog = Some(catalog);
    store.write_task(&record).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = TaskSetConfigOptionWorkflow::set_config_option(
        &api,
        TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: "gpt-5.5".to_string(),
            client_mutation_id: "mutation-1".into(),
        },
    )
    .unwrap();

    let stored = store.read_task("task-existing").unwrap();
    let catalog = stored.config_options_catalog.expect("stored catalog");
    assert_eq!(catalog.options[0].current_value, "gpt-5.5");
    assert_eq!(stored.model_id.as_deref(), Some("gpt-5.5"));
    assert_eq!(snapshot.agent_config.options[0].current_value, "gpt-5.5");
    assert_eq!(snapshot.agent_config.options[0].values.len(), 2);
}

#[test]
fn set_config_option_projects_catalog_missing_key_as_unsupported_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.config_options = config_catalog("gpt-5").current_values();
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = TaskSetConfigOptionWorkflow::set_config_option(
        &api,
        TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "custom".into(),
            value: "enabled".to_string(),
            client_mutation_id: "mutation-1".into(),
        },
    )
    .unwrap();

    assert_eq!(snapshot.agent_config.options.len(), 2);
    let fallback = snapshot
        .agent_config
        .options
        .iter()
        .find(|option| option.config_id.as_str() == "custom")
        .expect("fallback option");
    assert_eq!(fallback.label, "custom");
    assert_eq!(fallback.current_value, "enabled");
    assert!(fallback.values.is_empty());
}

#[test]
fn set_config_option_applies_to_running_task_live_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.config_options = config_catalog("gpt-5").current_values();
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let mut record = store.read_task("task-existing").unwrap();
    record.status = TaskStatus::Active;
    record.active_turn_id = Some("turn-active".to_string());
    record.agent_session_id = Some("session-active".to_string());
    store.write_task(&record).unwrap();

    let snapshot = TaskSetConfigOptionWorkflow::set_config_option(
        &api,
        TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: "gpt-5.5".to_string(),
            client_mutation_id: "mutation-1".into(),
        },
    )
    .unwrap();

    assert_eq!(
        agent.session_config_updates.lock().unwrap().as_slice(),
        [(
            "session-active".to_string(),
            "model".to_string(),
            "gpt-5.5".to_string()
        )]
    );
    let stored = store.read_task("task-existing").unwrap();
    assert_eq!(
        stored.config_options.get("model"),
        Some(&"gpt-5.5".to_string())
    );
    assert_eq!(stored.model_id.as_deref(), Some("gpt-5.5"));
    assert_ne!(stored.updated_at, "2026-01-01T00:00:00.000Z");
    assert_eq!(stored.last_activity, "2026-01-01T00:00:00.000Z");
    assert_eq!(snapshot.agent_config.options[0].current_value, "gpt-5.5");
}

#[test]
fn set_config_option_applies_to_prepared_task_native_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-prepared", "/workspace/app");
    record.agent_session_id = Some("session-prepared".to_string());
    record.config_options = config_catalog("gpt-5").current_values();
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    TaskSetConfigOptionWorkflow::set_config_option(
        &api,
        TaskSetConfigOptionParams {
            task_id: "task-prepared".into(),
            config_id: "model".into(),
            value: "gpt-5.5".to_string(),
            client_mutation_id: "mutation-prepared".into(),
        },
    )
    .unwrap();

    assert_eq!(
        agent.session_config_updates.lock().unwrap().as_slice(),
        [(
            "session-prepared".to_string(),
            "model".to_string(),
            "gpt-5.5".to_string()
        )]
    );
}

#[test]
fn discard_tombstones_empty_pre_send_task_and_returns_navigation() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/workspace/app");
    draft.first_prompt_sent = false;
    store.write_task(&draft).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let navigation = api
        .discard(TaskDiscardParams {
            task_id: "task-draft".into(),
        })
        .unwrap();

    assert!(store.read_task("task-draft").unwrap().tombstoned);
    assert_eq!(navigation.tasks.len(), 1);
    assert!(navigation
        .tasks
        .iter()
        .any(|task| task.task_id.as_str() == "task-existing"));
}

#[test]
fn discard_rejects_task_after_first_prompt_was_sent() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-existing", "/workspace/app"))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .discard(TaskDiscardParams {
            task_id: "task-existing".into(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert!(!store.read_task("task-existing").unwrap().tombstoned);
}

#[test]
fn discard_rejects_task_with_chat_history_even_before_prompt_flag() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.first_prompt_sent = false;
    store.write_task(&record).unwrap();
    append_old_completed_turn(&store, "task-existing");
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .discard(TaskDiscardParams {
            task_id: "task-existing".into(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert!(!store.read_task("task-existing").unwrap().tombstoned);
}

#[test]
fn discard_rejects_running_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.first_prompt_sent = false;
    record.status = TaskStatus::Active;
    record.active_turn_id = Some("turn-active".to_string());
    store.write_task(&record).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .discard(TaskDiscardParams {
            task_id: "task-existing".into(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert!(!store.read_task("task-existing").unwrap().tombstoned);
}

#[test]
fn discard_rejects_tombstoned_historical_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.tombstoned = true;
    store.write_task(&record).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .discard(TaskDiscardParams {
            task_id: "task-existing".into(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
}

#[test]
fn send_rejects_tombstoned_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/workspace/app");
    record.first_prompt_sent = false;
    record.tombstoned = true;
    store.write_task(&record).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let error = api
        .send(send_params("task-existing", 1, "send-1", "hello"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
    assert!(store.read_messages("task-existing").unwrap().is_empty());
}

#[test]
fn archiving_task_does_not_refresh_last_activity() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut older = task_record("task-old", "/workspace/app");
    older.last_activity = "2026-01-01T00:00:00.000Z".to_string();
    older.updated_at = older.last_activity.clone();
    let mut newer_archived = task_record("task-newer-archived", "/workspace/app");
    newer_archived.last_activity = "2026-02-01T00:00:00.000Z".to_string();
    newer_archived.updated_at = newer_archived.last_activity.clone();
    newer_archived.archived = true;
    store.write_task(&older).unwrap();
    store.write_task(&newer_archived).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.set_archived(TaskSetArchivedParams {
        task_id: "task-old".into(),
        archived: true,
    })
    .unwrap();

    let archived = store.list_archived_tasks().unwrap();
    assert_eq!(archived[0].task_id, "task-newer-archived");
    let archived_old = archived
        .iter()
        .find(|task| task.task_id == "task-old")
        .expect("archived old task");
    assert_ne!(archived_old.updated_at, "2026-01-01T00:00:00.000Z");
    assert_eq!(archived_old.last_activity, "2026-01-01T00:00:00.000Z");
}

fn task_record(task_id: &str, workspace_root: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: "Existing".to_string(),
        agent_title: None,
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace_root.to_string(),
        first_prompt_sent: true,
        agent_session_id: None,
        active_turn_id: None,
        archived: false,
        tombstoned: false,
        revision: 1,
        config_options: Default::default(),
        config_options_catalog: None,
        agent_commands_catalog: None,
        model_id: None,
        preparation: TaskPreparationRecord::Ready,
    }
}

fn send_params(task_id: &str, revision: u64, key: &str, text: &str) -> TaskSendParams {
    TaskSendParams {
        task_id: task_id.into(),
        idempotency_key: key.into(),
        task_revision: revision,
        message: ComposerMessage {
            text: Some(text.to_string()),
            attachments: Vec::new(),
        },
    }
}

fn append_old_completed_turn(store: &Store, task_id: &str) {
    store
        .append_message(
            task_id,
            ChatMessage {
                cursor: "m:1".to_string(),
                identity: "send:old".to_string(),
                message_type: "user".to_string(),
                message_id: "message_old_user".to_string(),
                message: NormalizedMessage::User {
                    id: "send:old".to_string(),
                    text: "old prompt".to_string(),
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                    attachments: Vec::new(),
                },
            },
        )
        .unwrap();
    store
        .append_message(
            task_id,
            ChatMessage {
                cursor: "m:2".to_string(),
                identity: "turn:turn_old".to_string(),
                message_type: "activity".to_string(),
                message_id: "message_old_turn".to_string(),
                message: NormalizedMessage::Activity {
                    id: "turn:turn_old".to_string(),
                    title: "Working".to_string(),
                    status: ActivityStatus::Completed,
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                    collapsed: true,
                    steps: vec![ActivityStep::Text {
                        text: "Done".to_string(),
                        level: Some("info".to_string()),
                    }],
                },
            },
        )
        .unwrap();
}

fn append_running_turn(store: &Store, task_id: &str, turn_id: &str) {
    store
        .append_message(
            task_id,
            ChatMessage {
                cursor: "m:running".to_string(),
                identity: format!("turn:{turn_id}"),
                message_type: "activity".to_string(),
                message_id: "message_running_turn".to_string(),
                message: NormalizedMessage::Activity {
                    id: format!("turn:{turn_id}"),
                    title: "Working".to_string(),
                    status: ActivityStatus::Running,
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                    collapsed: true,
                    steps: Vec::new(),
                },
            },
        )
        .unwrap();
}

#[derive(Default)]
struct RecordingAgent {
    starts: AtomicUsize,
    loads: AtomicUsize,
    resumes: AtomicUsize,
    prompts: AtomicUsize,
    attaches: AtomicUsize,
    cancels: AtomicUsize,
    closes: AtomicUsize,
    block_start: AtomicBool,
    block_attach: AtomicBool,
    block_resume: AtomicBool,
    config_catalog: Option<ConfigOptionsCatalog>,
    commands_catalog: Option<AgentCommandsCatalog>,
    listed_sessions: Mutex<Vec<AgentListedSession>>,
    replayed_messages: Mutex<Vec<NormalizedMessage>>,
    fail_start: bool,
    fail_attach: bool,
    fail_start_once: AtomicBool,
    fail_load_once_with_already_active: AtomicBool,
    resume_after_restart_unavailable: bool,
    resume_session_missing: bool,
    load_start_timeout: bool,
    loaded_session_id: Option<String>,
    block_prompt: bool,
    block_first_prompt: bool,
    complete_first_prompt_after_steering: usize,
    block_steering_prompts: bool,
    released_steering_prompts: AtomicUsize,
    completed_prompts: AtomicUsize,
    prompt_calls: Mutex<Vec<(String, String)>>,
    session_config_updates: Mutex<Vec<(String, String, String)>>,
    start_config_options: Mutex<Vec<Option<serde_json::Value>>>,
}

impl AgentRuntime for RecordingAgent {
    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        let mut sessions = self.listed_sessions.lock().unwrap().clone();
        for session in &mut sessions {
            if session.cwd.is_empty() {
                session.cwd = request.cwd.clone();
            }
        }
        Ok(AgentListSessionsResult {
            agent_id: request.agent_id,
            sessions,
            next_cursor: None,
        })
    }

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.start_config_options
            .lock()
            .unwrap()
            .push(request.config_options.clone());
        while self.block_start.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.fail_start {
            return Err(RuntimeError::NotReady("agent failed to start".to_string()));
        }
        if self.fail_start_once.swap(false, Ordering::SeqCst) {
            return Err(RuntimeError::NotReady(
                "ACP session start cancelled".to_string(),
            ));
        }
        let session = AgentSession::new("recorded-session");
        Ok(match &self.config_catalog {
            Some(catalog) => session.with_config_options(catalog),
            None => session,
        })
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        self.resumes.fetch_add(1, Ordering::SeqCst);
        while self.block_resume.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.resume_session_missing {
            return Err(RuntimeError::Internal(
                "ACP session is missing from the runtime".to_string(),
            ));
        }
        if self.resume_after_restart_unavailable {
            return Err(RuntimeError::CapabilityMissing(
                "acp_session_resume_after_runtime_restart".to_string(),
            ));
        }
        Ok(AgentSession::new(request.session_id))
    }

    fn load_session(&self, request: AgentSessionLoad) -> Result<AgentLoadedSession, RuntimeError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        if self.load_start_timeout {
            return Err(RuntimeError::NotReady(
                "ACP session start timed out".to_string(),
            ));
        }
        if self
            .fail_load_once_with_already_active
            .swap(false, Ordering::SeqCst)
        {
            return Err(RuntimeError::InvalidParams(
                "agent_session_id already active".to_string(),
            ));
        }
        Ok(AgentLoadedSession {
            session: AgentSession::new(
                self.loaded_session_id
                    .clone()
                    .unwrap_or_else(|| request.session_id.clone()),
            ),
            replayed_messages: self.replayed_messages.lock().unwrap().clone(),
        })
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        let prompt_number = self.prompts.fetch_add(1, Ordering::SeqCst) + 1;
        self.prompt_calls
            .lock()
            .unwrap()
            .push((prompt.session_id.clone(), prompt.text.clone()));
        while !prompt.cancellation.is_cancelled() {
            let first_prompt_is_blocked = self.block_first_prompt
                && prompt_number == 1
                && (self.complete_first_prompt_after_steering == 0
                    || self.completed_prompts.load(Ordering::SeqCst)
                        < self.complete_first_prompt_after_steering);
            if !self.block_prompt && !first_prompt_is_blocked {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        while self.block_steering_prompts
            && prompt_number > 1
            && self.released_steering_prompts.load(Ordering::SeqCst) < prompt_number - 1
            && !prompt.cancellation.is_cancelled()
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.completed_prompts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.session_config_updates.lock().unwrap().push((
            request.session_id,
            request.config_id,
            request.value.clone(),
        ));
        Ok(config_catalog(&request.value))
    }

    fn attach_session_event_sink(
        &self,
        _session_id: &str,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.attaches.fetch_add(1, Ordering::SeqCst);
        while self.block_attach.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.fail_attach {
            return Err(RuntimeError::NotReady(
                "session event attachment failed".to_string(),
            ));
        }
        if let Some(catalog) = &self.commands_catalog {
            sink.commands_changed(catalog.clone())?;
        }
        Ok(())
    }

    fn cancel_session(&self, _session_id: &str) -> Result<(), RuntimeError> {
        self.cancels.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn close_session(&self, _session_id: &str) -> Result<(), RuntimeError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct SecretResolvingAgent {
    resolved: Arc<Mutex<Option<HashMap<String, String>>>>,
}

impl AgentRuntime for SecretResolvingAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        let resolver = request
            .secret_resolver
            .ok_or_else(|| RuntimeError::NotReady("task secret resolver missing".to_string()))?;
        let values = resolver.resolve_secret_env(&request.agent_id, &["TOKEN".to_string()])?;
        *self.resolved.lock().unwrap() = Some(values);
        Ok(AgentSession::new("secret-session"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[derive(Default)]
struct PagedSessionAgent {
    requested_cursors: Mutex<Vec<Option<String>>>,
}

impl PagedSessionAgent {
    fn requested_cursors(&self) -> Vec<Option<String>> {
        self.requested_cursors.lock().unwrap().clone()
    }
}

impl AgentRuntime for PagedSessionAgent {
    fn list_sessions(
        &self,
        request: crate::agent::AgentListSessionsRequest,
    ) -> Result<crate::protocol::model::AgentListSessionsResult, RuntimeError> {
        self.requested_cursors
            .lock()
            .unwrap()
            .push(request.cursor.clone());
        let sessions = match request.cursor.as_deref() {
            None => Vec::new(),
            Some("page-2") => vec![crate::protocol::model::AgentListedSession {
                session_id: "matching-session".to_string(),
                cwd: request.cwd,
                title: Some("Matching project".to_string()),
                last_activity: None,
                updated_at: None,
            }],
            _ => Vec::new(),
        };
        let next_cursor = match request.cursor.as_deref() {
            None => Some("page-2".to_string()),
            Some("page-2") => Some("page-3".to_string()),
            _ => None,
        };
        Ok(crate::protocol::model::AgentListSessionsResult {
            agent_id: request.agent_id,
            sessions,
            next_cursor,
        })
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("paged-session"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}

fn config_catalog(current_value: &str) -> ConfigOptionsCatalog {
    ConfigOptionsCatalog {
        agent_id: "codex".to_string(),
        status: ConfigOptionsStatus::Ready,
        options: vec![ConfigOption {
            id: "model".to_string(),
            label: "Model".to_string(),
            description: Some("Select model".to_string()),
            category: Some(ConfigOptionCategory::Model),
            current_value: current_value.to_string(),
            values: vec![
                ConfigOptionValue {
                    id: "gpt-5".to_string(),
                    label: "GPT 5".to_string(),
                    description: Some("Stable".to_string()),
                    group_id: None,
                    group_label: None,
                },
                ConfigOptionValue {
                    id: "gpt-5.5".to_string(),
                    label: "GPT 5.5".to_string(),
                    description: Some("Frontier".to_string()),
                    group_id: None,
                    group_label: None,
                },
            ],
        }],
    }
}

fn command_catalog() -> AgentCommandsCatalog {
    AgentCommandsCatalog {
        commands: vec![AgentCommand {
            name: "web".to_string(),
            description: "Search the web".to_string(),
            input_hint: Some("query".to_string()),
        }],
    }
}

struct ConfigMutatingStartAgent {
    store: Store,
}

impl AgentRuntime for ConfigMutatingStartAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        let mut task = self.store.read_task("task-existing")?;
        task.config_options
            .insert("model".to_string(), "new-model".to_string());
        task.revision += 1;
        self.store.write_task(&task)?;
        Ok(AgentSession::new("mutating-session"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(condition());
}
