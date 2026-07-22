use super::*;
use crate::agent::registry::{AgentCatalogRecord, AgentRegistry};
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentEventSink, AgentListSessionsRequest, AgentLoadedSession, AgentPrompt, AgentRuntime,
    AgentSession, AgentSessionEventSink, AgentSessionKey, AgentSessionLoad, AgentSessionResume,
    AgentSessionSetConfigOptionRequest, AgentSessionStart,
};
use crate::attachment_runtime::AttachmentRuntimeError;
use crate::client_lifecycle::{AppServerTime, ConnectionId, Delivery};
use crate::projects::{project_id_for_workspace, ProjectTaskContext, StorageProjectResolver};
use crate::protocol::model::{
    ActivityStatus, ActivityStep, ActivityToolContent, ActivityToolDetails, AgentCommand,
    AgentCommandsCatalog, AgentListSessionsResult, AgentListedSession, AgentMessagePart,
    AgentMessageRole, Attachment, ChatMessage, ConfigOption, ConfigOptionCategory,
    ConfigOptionCurrentValue, ConfigOptionKind, ConfigOptionValue, ConfigOptionsCatalog,
    ConfigOptionsStatus, InterruptionReason, IsolationKind, NormalizedMessage, TaskStatus,
};
use crate::server_requests::{ServerRequestAnswer, ServerRequestRuntime};
use crate::snapshots::task_snapshot::project_stored_task_snapshot;
use crate::storage::records::{
    TaskLifecycle, TaskPreparationRecord, TaskRecord, TaskTitle, TaskTitleSource,
};
use crate::storage::Store;
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::mutation::TaskMutationResult;
use openaide_app_server_protocol::agent::AgentListSessionsParams;
use openaide_app_server_protocol::ids::{AgentId, ClientInstanceId, ProjectId, TaskId};
use openaide_app_server_protocol::snapshot::{
    AgentConfigOptionCurrentValue, LiveSessionDataState, MessagePart, TaskHistorySyncSnapshot,
    TaskPreparationSnapshot, TaskSendCapabilityState, TaskStatus as ProtocolTaskStatus,
    TaskTitleSource as ProtocolTaskTitleSource,
};
use openaide_app_server_protocol::support::SupportRecoverStuckSessionsParams;
use openaide_app_server_protocol::task::{
    ComposerImage, ComposerMessage, TaskAcquireParams, TaskAdoptNativeSessionParams,
    TaskCancelParams, TaskMarkReadParams, TaskOpenParams, TaskReleaseParams, TaskSendParams,
    TaskSetArchivedParams, TaskSetConfigOptionParams,
};
use openaide_app_server_protocol::workspace::WorkspaceListDirectoryParams;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn protocol_config_id(value: &str) -> AgentConfigOptionCurrentValue {
    AgentConfigOptionCurrentValue::Id {
        value: value.to_string(),
    }
}

fn protocol_value_id(value: &AgentConfigOptionCurrentValue) -> Option<&str> {
    match value {
        AgentConfigOptionCurrentValue::Id { value } => Some(value),
        AgentConfigOptionCurrentValue::Boolean { .. } => None,
    }
}

fn task_config_id<'a>(task: &'a TaskRecord, id: &str) -> Option<&'a str> {
    task.config_options_catalog
        .as_ref()?
        .options
        .iter()
        .find(|option| option.id == id)?
        .current_value
        .as_id()
}

#[test]
fn acquire_in_worktree_persists_workspace_identity_without_splitting_project() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).unwrap();
    git(&project_root, &["init", "-b", "main"]);
    git(&project_root, &["config", "user.name", "OpenAIDE Test"]);
    git(
        &project_root,
        &["config", "user.email", "test@example.invalid"],
    );
    std::fs::write(project_root.join("README.md"), "test\n").unwrap();
    git(&project_root, &["add", "README.md"]);
    git(&project_root, &["commit", "-m", "initial"]);

    let store = Store::open(temp.path().join("state")).unwrap();
    let project_root_text = project_root.to_string_lossy().to_string();
    store
        .write_task(&task_record("task-project-anchor", &project_root_text))
        .unwrap();
    let manager = crate::worktrees::WorktreeManager::new(store.clone());
    let repository = manager.refresh_project(&project_root).unwrap().unwrap();
    let created = manager
        .create(crate::worktrees::CreateWorktree {
            repository_id: repository.repository.repository_id,
            source_project_root: project_root.clone(),
            name: "Sidebar scrolling".to_string(),
            base: crate::worktrees::WorktreeBase::CurrentHead,
            branch: None,
        })
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
        .acquire_in_worktree_for_client(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            openaide_app_server_protocol::task::TaskAcquireInWorktreeParams {
                project_id: project_id_for_workspace(&project_root_text),
                agent_id: AgentId::from("codex"),
                worktree_id: created.worktree_id.clone(),
            },
        )
        .unwrap();

    assert_eq!(snapshot.task.worktree_id, Some(created.worktree_id.clone()));
    assert_eq!(
        snapshot.task.project_id,
        project_id_for_workspace(&project_root_text)
    );
    let record = store.read_task(snapshot.task.task_id.as_str()).unwrap();
    assert_eq!(
        record.worktree_id.as_deref(),
        Some(created.worktree_id.as_str())
    );
    assert_ne!(record.workspace_root, project_root_text);
    assert_eq!(
        record.project_root.as_deref(),
        Some(project_root_text.as_str())
    );
}

#[test]
fn send_rejects_a_task_after_its_worktree_folder_disappears() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("missing-worktree");
    let workspace_text = workspace.to_string_lossy().to_string();
    let mut record = task_record("task-missing-worktree", &workspace_text);
    record.worktree_id = Some("worktree_missing".to_string());
    record.project_root = Some(temp.path().to_string_lossy().to_string());
    std::fs::remove_dir_all(&workspace).unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
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
        .send(send_params(&record.task_id, "continue"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert_eq!(
        error.message,
        "Task workspace is unavailable. Restore it before sending."
    );
    assert!(!store.read_task(&record.task_id).unwrap().tombstoned);
}

fn git(cwd: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn create_persists_idle_task_without_prompt_or_turn() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.title = None;
    record.lifecycle = test_new_task_lifecycle();
    store.write_task(&record).unwrap();
    let project_id = project_id_for_workspace("/tmp/openaide-unit-workspace/app");
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = api
        .create_for_test(TaskAcquireParams {
            project_id,
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .unwrap();
    assert_eq!(snapshot.task.title, None);

    let record = store.read_task(snapshot.task.task_id.as_str()).unwrap();
    assert_eq!(record.status, TaskStatus::Inactive);
    assert!(matches!(record.lifecycle, TaskLifecycle::New { .. }));
    assert_eq!(record.active_turn_id, None);
    assert!(store.read_messages(&record.task_id).unwrap().is_empty());
    assert_eq!(snapshot.chat.items.len(), 0);
}

#[test]
fn create_reopens_the_existing_draft_for_the_same_project_and_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut project_anchor = task_record("task-project-anchor", "/tmp/openaide-unit-workspace/app");
    project_anchor.lifecycle = TaskLifecycle::Visible;
    store.write_task(&project_anchor).unwrap();
    let project_id = project_id_for_workspace("/tmp/openaide-unit-workspace/app");
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let params = TaskAcquireParams {
        project_id,
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    };

    let first = api.create_for_test(params.clone()).unwrap();
    let reopened = api.create_for_test(params).unwrap();

    assert_eq!(reopened.task.task_id, first.task.task_id);
    assert_eq!(store.task_record_count().unwrap(), 2);
}

#[test]
fn different_clients_get_distinct_new_tasks() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-project-anchor",
            "/tmp/openaide-unit-workspace/app",
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
    let params = TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    };

    let first = api
        .acquire_for_client(&ClientInstanceId::from("client-a"), params.clone())
        .unwrap();
    let second = api
        .acquire_for_client(&ClientInstanceId::from("client-b"), params)
        .unwrap();

    assert_ne!(first.task.task_id, second.task.task_id);
}

#[test]
fn new_task_context_change_is_a_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-project-anchor",
            "/tmp/openaide-unit-workspace/app",
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
    let client = ClientInstanceId::from("client-a");
    let params = |agent_id| TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from(agent_id),
        workspace_root: None,
    };
    api.acquire_for_client(&client, params("codex")).unwrap();

    let error = api
        .acquire_for_client(&client, params("opencode"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
}

#[test]
fn concurrent_create_for_one_client_resolves_one_new_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-project-anchor",
            "/tmp/openaide-unit-workspace/app",
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
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let api = api.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            api.acquire_for_client(
                &ClientInstanceId::from("client-a"),
                TaskAcquireParams {
                    project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
                    agent_id: AgentId::from("codex"),
                    workspace_root: None,
                },
            )
            .unwrap()
            .task
            .task_id
        }));
    }
    barrier.wait();
    let first = workers.remove(0).join().unwrap();
    let second = workers.remove(0).join().unwrap();

    assert_eq!(first, second);
    assert_eq!(
        store
            .list_all_task_records()
            .unwrap()
            .into_iter()
            .filter(|task| matches!(task.lifecycle, TaskLifecycle::New { .. }))
            .count(),
        1
    );
}

#[test]
fn released_prepared_task_is_reused_by_another_client() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-project-anchor",
            "/tmp/openaide-unit-workspace/app",
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
    let params = TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    };
    let first = api
        .acquire_for_client(&ClientInstanceId::from("client-a"), params.clone())
        .unwrap();
    wait_until(|| {
        matches!(
            store
                .read_task(first.task.task_id.as_str())
                .unwrap()
                .preparation,
            TaskPreparationRecord::Ready
        )
    });
    api.release_for_client(
        &ClientInstanceId::from("client-a"),
        TaskReleaseParams {
            task_id: first.task.task_id.clone(),
        },
    )
    .unwrap();

    let second = api
        .acquire_for_client(&ClientInstanceId::from("client-b"), params)
        .unwrap();

    assert_eq!(first.task.task_id, second.task.task_id);
    assert!(
        !store
            .read_task(first.task.task_id.as_str())
            .unwrap()
            .tombstoned
    );
}

#[test]
fn restart_clears_prepared_task_lease_and_preserves_the_task_for_reuse() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-project-anchor",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let params = TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    };
    let first_api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let first = first_api
        .acquire_for_client(
            &ClientInstanceId::from("client-before-restart"),
            params.clone(),
        )
        .unwrap();
    wait_until(|| {
        matches!(
            store
                .read_task(first.task.task_id.as_str())
                .unwrap()
                .preparation,
            TaskPreparationRecord::Ready
        )
    });
    drop(first_api);

    let restarted_api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let reused = restarted_api
        .acquire_for_client(&ClientInstanceId::from("client-after-restart"), params)
        .unwrap();

    assert_eq!(reused.task.task_id, first.task.task_id);
}

#[test]
fn new_task_cannot_be_archived_or_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-project-anchor",
            "/tmp/openaide-unit-workspace/app",
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
    let params = TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    };
    let created = api.create_for_test(params.clone()).unwrap();

    let error = api
        .set_archived_for_test(TaskSetArchivedParams {
            task_id: created.task.task_id.clone(),
            archived: true,
        })
        .unwrap_err();
    let reopened = api.create_for_test(params).unwrap();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert_eq!(reopened.task.task_id, created.task.task_id);
}

#[test]
fn startup_isolates_a_malformed_task_and_keeps_unrelated_tasks_available() {
    struct FixedProjectResolver;

    impl ProjectResolver for FixedProjectResolver {
        fn resolve_task_context(
            &self,
            project_id: &ProjectId,
        ) -> Result<ProjectTaskContext, ProtocolError> {
            Ok(ProjectTaskContext {
                project_id: project_id.clone(),
                workspace_root: "/tmp/openaide-unit-workspace/app".to_string(),
                label: "app".to_string(),
                isolation: IsolationKind::Local,
            })
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "corrupt-task",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    drop(store);
    corrupt_last_byte(
        &temp
            .path()
            .join("task-store-v1/tasks/corrupt-task/task.journal"),
    );
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(FixedProjectResolver),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .expect("one malformed Task must not prevent runtime startup");
    let acquired = api
        .create_for_test(TaskAcquireParams {
            project_id: ProjectId::from("project-after-corruption"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("malformed Task must not block unrelated acquisition");

    assert!(store.read_task("corrupt-task").is_err());
    assert_ne!(acquired.task.task_id.as_str(), "corrupt-task");
    assert!(store.list_all_task_records_strict().is_err());
}

#[test]
fn startup_does_not_replay_inactive_task_chat_during_volatile_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "inactive-corrupt-chat",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    drop(store);
    corrupt_first_payload_byte(
        &temp
            .path()
            .join("task-store-v1/tasks/inactive-corrupt-chat/task.journal"),
    );

    let store = Store::open(temp.path().to_path_buf()).unwrap();
    TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .expect("inactive Task Chat must stay lazy during volatile recovery");

    assert!(
        store.read_task("inactive-corrupt-chat").is_err(),
        "opening the Task must still surface its corrupt Chat"
    );
}

fn corrupt_last_byte(path: &std::path::Path) {
    use std::io::{Read, Seek, Write};
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.seek(std::io::SeekFrom::End(-1)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    file.seek(std::io::SeekFrom::End(-1)).unwrap();
    file.write_all(&[byte[0] ^ 0xff]).unwrap();
}

fn corrupt_first_payload_byte(path: &std::path::Path) {
    use std::io::{Read, Seek, Write};
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    // Header (10 bytes) and frame length (8 bytes) precede the JSON payload.
    file.seek(std::io::SeekFrom::Start(18)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    file.seek(std::io::SeekFrom::Start(18)).unwrap();
    file.write_all(&[byte[0] ^ 0xff]).unwrap();
    file.sync_all().unwrap();
}

#[test]
fn create_reactivates_the_reused_draft_native_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/tmp/openaide-unit-workspace/app");
    draft.lifecycle = test_new_task_lifecycle();
    draft.agent_session_id = Some("session-draft".to_string());
    draft.preparation = TaskPreparationRecord::Ready;
    draft.config_options_catalog = Some(mode_config_catalog("agent"));
    store.write_task(&draft).unwrap();
    let agent = Arc::new(RecordingAgent {
        config_catalog: Some(config_catalog("gpt-5")),
        resume_config_catalog: Some(config_catalog("gpt-5")),
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

    api.create_for_test(TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
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
    let reopened = store.read_task("task-draft").unwrap();
    assert_eq!(task_config_id(&reopened, "model"), Some("gpt-5"));
    let updated = api
        .set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-draft".into(),
            config_id: "model".into(),
            value: protocol_config_id("gpt-5.5"),
            client_mutation_id: "reactivated-draft-config".into(),
        })
        .unwrap();
    assert_eq!(
        protocol_value_id(&updated.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
}

#[test]
fn create_replaces_a_persisted_draft_catalog_with_fresh_agent_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/tmp/openaide-unit-workspace/app");
    draft.lifecycle = test_new_task_lifecycle();
    draft.config_options_catalog = Some(mode_config_catalog("full-access"));
    store.write_task(&draft).unwrap();
    let agent = Arc::new(RecordingAgent {
        config_catalog: Some(mode_config_catalog("agent-full-access")),
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

    api.create_for_test(TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    })
    .unwrap();

    wait_until(|| {
        matches!(
            store.read_task("task-draft").unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });
    let recovered = store.read_task("task-draft").unwrap();
    assert_eq!(
        recovered
            .config_options_catalog
            .expect("fresh catalog")
            .options[0]
            .current_value
            .as_id(),
        Some("agent-full-access")
    );
}

#[test]
fn draft_preparation_keeps_catalogs_delivered_immediately_when_the_sink_attaches() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-project-anchor",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(ImmediatePreparationCatalogAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .unwrap();
    let task_id = created.task.task_id.clone();
    wait_until(|| {
        matches!(
            store.read_task(task_id.as_str()).unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });

    let prepared = api
        .open_for_test(TaskOpenParams {
            task_id: task_id.clone(),
        })
        .unwrap();

    assert_eq!(prepared.agent_config.state, LiveSessionDataState::Ready);
    assert_eq!(
        protocol_value_id(&prepared.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
    assert_eq!(prepared.agent_commands.state, LiveSessionDataState::Ready);
    assert_eq!(prepared.agent_commands.commands[0].name, "web");
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
    let mut record = task_record("task-stale-turn", "/tmp/openaide-unit-workspace/app");
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
                status: ActivityStatus::Interrupted,
                ..
            }
        )
    }));
    assert!(messages.iter().any(|message| {
        matches!(
            message.chat.message,
            NormalizedMessage::Interruption {
                reason: InterruptionReason::BackendUnavailable,
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
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .unwrap();

    wait_until(|| agent.starts.load(Ordering::SeqCst) == 1);
    let preparing_record = store.read_task(snapshot.task.task_id.as_str()).unwrap();
    let too_early = api
        .send(send_params(snapshot.task.task_id.as_str(), "too soon"))
        .unwrap_err();
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
    assert_eq!(too_early.code, ProtocolErrorCode::Conflict);
    assert!(store
        .read_messages(snapshot.task.task_id.as_str())
        .unwrap()
        .is_empty());

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
        .open_for_test(TaskOpenParams {
            task_id: snapshot.task.task_id.clone(),
        })
        .unwrap();

    assert!(matches!(ready.preparation, TaskPreparationSnapshot::Ready));
    assert_eq!(ready.agent_config.state, LiveSessionDataState::Ready);
    assert_eq!(
        protocol_value_id(&ready.agent_config.options[0].current_value),
        Some("gpt-5")
    );
    assert_eq!(ready.agent_commands.state, LiveSessionDataState::Ready);
    assert_eq!(ready.agent_commands.commands[0].name, "web");
    let accepted = api
        .send(send_params(snapshot.task.task_id.as_str(), "ready now"))
        .unwrap();
    assert!(accepted.turn_id.as_str().starts_with("turn_"));
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
fn create_projects_native_session_start_failure_into_send_readiness() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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

    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
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
        .open_for_test(TaskOpenParams {
            task_id: created.task.task_id.clone(),
        })
        .unwrap();
    let rejected = api
        .send(send_params(created.task.task_id.as_str(), "do not commit"))
        .unwrap_err();

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
        TaskPreparationRecord::Failed { ref message, .. }
            if message.contains("agent failed to start")
    ));
    assert_eq!(failed_record.agent_session_id, None);
    assert_eq!(rejected.code, ProtocolErrorCode::Internal);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    assert!(store
        .read_messages(created.task.task_id.as_str())
        .unwrap()
        .is_empty());
}

#[test]
fn task_preparation_resolves_custom_agent_secrets_through_typed_server_requests() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("custom.agent"),
            workspace_root: None,
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
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        fail_attach: true,
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

    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
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

    assert_eq!(failed.agent_session_id.as_deref(), Some("recorded-session"));
    assert_eq!(failed.config_options_catalog, None);
    assert_eq!(failed.agent_commands_catalog, None);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(agent.closes.load(Ordering::SeqCst), 1);
}

#[test]
fn first_send_reuses_the_native_session_prepared_during_create() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .unwrap();
    assert_eq!(created.task.title, None);
    wait_until(|| {
        matches!(
            store
                .read_task(created.task.task_id.as_str())
                .unwrap()
                .preparation,
            TaskPreparationRecord::Ready
        )
    });
    api.send(send_params(created.task.task_id.as_str(), "hello"))
        .unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 2);
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
    assert_eq!(
        store
            .read_task(created.task.task_id.as_str())
            .unwrap()
            .title,
        TaskTitle::new("hello", TaskTitleSource::Prompt)
    );
}

#[test]
fn first_send_promotes_new_task_with_prompt_title() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(RecordingAgent::default()),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
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

    let accepted = api
        .send(send_params(
            created.task.task_id.as_str(),
            "  Explain why session titles are missing and repair their synchronization safely  ",
        ))
        .unwrap();

    let title = accepted
        .task
        .task
        .title
        .expect("first Send supplies a title");
    assert_eq!(
        title.value,
        "Explain why session titles are missing and repair their sync..."
    );
    assert_eq!(title.source, ProtocolTaskTitleSource::Prompt);
}

#[test]
fn acquire_returns_while_prepared_session_resume_is_blocked() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/tmp/openaide-unit-workspace/app");
    draft.lifecycle = test_new_task_lifecycle();
    draft.agent_session_id = Some("prepared-session".to_string());
    store.write_task(&draft).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_resume: AtomicBool::new(true),
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
    let acquired = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .unwrap();
    assert_eq!(acquired.task.task_id.as_str(), "task-draft");
    agent.block_resume.store(false, Ordering::SeqCst);
    wait_until(|| {
        matches!(
            store.read_task("task-draft").unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = api.send(send_params("task-draft", "hello"));
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
}

#[test]
fn resumed_identity_only_session_preserves_known_image_capability() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/tmp/openaide-unit-workspace/app");
    draft.lifecycle = test_new_task_lifecycle();
    draft.agent_session_id = Some("known-session".to_string());
    draft.supports_image_input = true;
    store.write_task(&draft).unwrap();
    let agent = Arc::new(RecordingAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.create_for_test(TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    })
    .unwrap();
    wait_until(|| {
        matches!(
            store.read_task("task-draft").unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });

    assert!(store.read_task("task-draft").unwrap().supports_image_input);
}

#[test]
fn reacquiring_replaces_a_prepared_task_whose_native_session_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/tmp/openaide-unit-workspace/app");
    draft.lifecycle = test_new_task_lifecycle();
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
    let params = TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    };
    let stale = api.create_for_test(params.clone()).unwrap();
    assert_eq!(stale.task.task_id.as_str(), "task-draft");
    wait_until(|| {
        matches!(
            store.read_task("task-draft").unwrap().preparation,
            TaskPreparationRecord::Failed { .. }
        )
    });

    let replacement = api.create_for_test(params).unwrap();
    let replacement_id = replacement.task.task_id.as_str().to_string();
    assert_ne!(replacement_id, "task-draft");
    wait_until(|| {
        matches!(
            store.read_task(&replacement_id).unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });

    assert!(store.read_task("task-draft").unwrap().tombstoned);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.read_task(&replacement_id).unwrap().agent_session_id,
        Some("recorded-session".to_string())
    );
}

#[test]
fn send_projects_agent_config_catalog_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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

    api.send(send_params("task-existing", "hello")).unwrap();
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
    assert_eq!(protocol_value_id(&option.current_value), Some("gpt-5"));
    assert_eq!(option.values.len(), 2);
    assert_eq!(option.values[1].value, "gpt-5.5");
    assert_eq!(option.values[1].label, "GPT 5.5");
}

#[test]
fn send_projects_agent_command_catalog_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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

    api.send(send_params("task-existing", "hello")).unwrap();
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
fn startup_marks_abandoned_preparation_failed_and_removes_it_from_the_pool() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-preparing", "/tmp/openaide-unit-workspace/app");
    task.lifecycle = test_new_task_lifecycle();
    task.preparation = TaskPreparationRecord::Preparing;
    task.agent_session_id = Some("bound-before-attach".to_string());
    task.config_options_catalog = Some(config_catalog("gpt-5.5"));
    task.agent_commands_catalog = Some(command_catalog());
    task.model_id = Some("gpt-5.5".to_string());
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
    assert!(record.tombstoned);
    assert_eq!(
        record.agent_session_id.as_deref(),
        Some("bound-before-attach")
    );
    assert_eq!(record.config_options_catalog, None);
    assert_eq!(record.agent_commands_catalog, None);
    assert_eq!(record.model_id.as_deref(), Some("gpt-5.5"));
    let rejected = api
        .send(send_params("task-preparing", "hello"))
        .unwrap_err();
    assert_eq!(rejected.code, ProtocolErrorCode::NotFound);
    assert!(store.read_messages("task-preparing").unwrap().is_empty());
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
        .create_for_test(TaskAcquireParams {
            project_id: ProjectId::from("project-missing"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
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
    let workspace_root = "/tmp/openaide-unit-workspace/new-app";
    std::fs::create_dir_all(workspace_root).unwrap();

    let snapshot = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: Some(workspace_root.to_string()),
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
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/other"),
            agent_id: AgentId::from("codex"),
            workspace_root: Some("/tmp/openaide-unit-workspace/new-app".to_string()),
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
        Arc::new(StorageProjectResolver::new(store.clone())),
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
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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

    let error = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("missing-agent"),
            workspace_root: None,
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::CapabilityUnavailable);
}

#[test]
fn list_agent_sessions_filters_already_adopted_native_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            cursor: None,
        })
        .unwrap();

    assert!(result.sessions.is_empty());
}

#[test]
fn list_agent_sessions_does_not_replace_owned_task_title_from_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.title = TaskTitle::new("Prompt fallback", TaskTitleSource::Prompt);
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: Some("Agent catalog title".to_string()),
            last_activity: None,
            updated_at: None,
        }]),
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

    let result = api
        .list_agent_sessions(AgentListSessionsParams {
            agent_id: AgentId::from("codex"),
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            cursor: None,
        })
        .unwrap();

    assert!(result.sessions.is_empty());
    assert_eq!(
        store.read_task("task-existing").unwrap().title,
        TaskTitle::new("Prompt fallback", TaskTitleSource::Prompt)
    );
}

#[test]
fn native_session_adoption_is_scoped_by_agent_not_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "project-a-task",
            "/tmp/openaide-unit-workspace/a",
        ))
        .unwrap();
    store
        .write_task(&task_record(
            "project-b-task",
            "/tmp/openaide-unit-workspace/b",
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "shared-native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/b".to_string(),
            title: Some("Shared session".to_string()),
            updated_at: None,
            last_activity: None,
        }]),
        ..RecordingAgent::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/a"),
        cursor: None,
    })
    .expect("discover shared session");
    let params = |_workspace: &str, agent_id: &str| TaskAdoptNativeSessionParams {
        agent_id: AgentId::from(agent_id),
        native_session_id: "shared-native-session".to_string(),
    };

    let adopted = api
        .adopt_native_session(params("/tmp/openaide-unit-workspace/a", "codex"))
        .expect("first Agent session owner");
    assert_eq!(
        adopted.lifecycle,
        openaide_app_server_protocol::snapshot::TaskLifecycle::Visible
    );
    assert_eq!(
        adopted.task.title,
        Some(openaide_app_server_protocol::snapshot::TaskTitle {
            value: "Shared session".to_string(),
            source: openaide_app_server_protocol::snapshot::TaskTitleSource::Agent,
        })
    );
    let listed = api
        .list_agent_sessions(AgentListSessionsParams {
            agent_id: AgentId::from("codex"),
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/b"),
            cursor: None,
        })
        .expect("list Agent sessions in another workspace");
    assert!(
        listed.sessions.is_empty(),
        "an Agent session owned in another workspace must not be offered for adoption"
    );
    let duplicate = api
        .adopt_native_session(params("/tmp/openaide-unit-workspace/b", "codex"))
        .expect("repeat adoption converges on the existing Task");
    assert_eq!(duplicate.task.task_id, adopted.task.task_id);
    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("opencode"),
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/b"),
        cursor: None,
    })
    .expect("discover same id for the other Agent");
    api.adopt_native_session(params("/tmp/openaide-unit-workspace/b", "opencode"))
        .expect("another Agent may reuse the same native session id");
}

#[test]
fn adopting_native_session_preserves_its_listed_activity_time() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "project-task",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: Some("Existing session".to_string()),
            last_activity: None,
            updated_at: Some("2026-01-02T03:04:05.000Z".to_string()),
        }]),
        ..RecordingAgent::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        cursor: None,
    })
    .unwrap();
    let adopted = api
        .adopt_native_session(TaskAdoptNativeSessionParams {
            agent_id: AgentId::from("codex"),
            native_session_id: "native-session".to_string(),
        })
        .unwrap();

    assert_eq!(adopted.task.last_activity, "2026-01-02T03:04:05.000Z");
}

#[test]
fn adopting_native_session_persists_replayed_tool_details_with_the_new_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "project-task",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: Some("Existing session".to_string()),
            last_activity: None,
            updated_at: None,
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::Activity {
            id: "activity-1".to_string(),
            title: "Edited a file".to_string(),
            status: ActivityStatus::Completed,
            created_at: "2026-01-02T03:04:05.000Z".to_string(),
            collapsed: true,
            steps: vec![ActivityStep::Tool {
                tool_call_id: Some("tool-1".to_string()),
                name: "edit".to_string(),
                status: ActivityStatus::Completed,
                input_summary: None,
                output_preview: None,
                detail_artifact_id: None,
                details: Some(Box::new(ActivityToolDetails {
                    locations: Vec::new(),
                    content: vec![ActivityToolContent::Diff {
                        path: "/tmp/openaide-unit-workspace/app/src/main.rs".to_string(),
                        old_text: Some("old".to_string()),
                        new_text: "new".to_string(),
                    }],
                    input: None,
                    output: None,
                })),
                permission_outcomes: Vec::new(),
            }],
        }]),
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

    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        cursor: None,
    })
    .expect("discover replayable session");
    let adopted = api
        .adopt_native_session(TaskAdoptNativeSessionParams {
            agent_id: AgentId::from("codex"),
            native_session_id: "native-session".to_string(),
        })
        .expect("adopt replayed session with tool details");

    assert_eq!(adopted.chat.items.len(), 1);
    let task_id = adopted.task.task_id.as_str();
    let stored = store.read_messages(task_id).unwrap();
    let NormalizedMessage::Activity { steps, .. } = &stored[0].chat.message else {
        panic!("expected replayed activity");
    };
    let ActivityStep::Tool {
        detail_artifact_id,
        details,
        ..
    } = &steps[0]
    else {
        panic!("expected replayed tool step");
    };
    assert!(detail_artifact_id.is_some());
    assert!(details.is_none());
}

#[test]
fn list_agent_sessions_hides_a_native_session_while_draft_ownership_is_committing() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_attach: AtomicBool::new(true),
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "recorded-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
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

    api.create_for_test(TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    })
    .unwrap();
    wait_until(|| agent.attaches.load(Ordering::SeqCst) == 1);

    let result = api
        .list_agent_sessions(AgentListSessionsParams {
            agent_id: AgentId::from("codex"),
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
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
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
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
fn list_agent_sessions_stops_when_empty_pages_cycle_between_cursors() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let agent = Arc::new(CyclingEmptySessionAgent::default());
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
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            cursor: None,
        })
        .expect("a cursor cycle is treated as exhausted history");

    assert!(result.sessions.is_empty());
    assert_eq!(result.next_cursor, None);
    assert_eq!(
        agent.requested_cursors(),
        vec![None, Some("page-2".to_string()), Some("page-3".to_string()),]
    );
}

#[test]
fn background_native_catalog_refresh_stops_when_a_page_adds_no_session_identity() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("missing-native-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(CyclingEmptySessionAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.refresh_native_session_catalogs().unwrap();
    assert_eq!(agent.requested_cursors(), vec![None, None]);
}

#[test]
fn background_native_catalog_refresh_does_not_replace_owned_task_title() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.title = TaskTitle::new("Prompt fallback", TaskTitleSource::Prompt);
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: Some("Agent catalog title".to_string()),
            last_activity: None,
            updated_at: None,
        }]),
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

    api.refresh_native_session_catalogs().unwrap();

    assert_eq!(
        store.read_task("task-existing").unwrap().title,
        TaskTitle::new("Prompt fallback", TaskTitleSource::Prompt)
    );
}

#[test]
fn native_catalog_refresh_requests_coalesce_with_one_trailing_run() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_list: AtomicBool::new(true),
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

    api.request_native_session_catalog_refresh();
    wait_until(|| agent.list_calls.load(Ordering::SeqCst) == 1);
    api.request_native_session_catalog_refresh();
    agent.block_list.store(false, Ordering::SeqCst);

    wait_until(|| !api.native_session_catalog().refreshing());
    assert_eq!(agent.list_calls.load(Ordering::SeqCst), 4);
}

#[test]
fn open_reloads_adopted_task_when_native_session_is_newer_than_cached_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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
                message_type: "agent_message".to_string(),
                message_id: "cached_message".to_string(),
                message: NormalizedMessage::AgentMessage {
                    id: "cached:stale".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: "Stale cached history.".to_string(),
                    }],
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                },
            },
        )
        .unwrap();
    let mut task = store.read_task("task-existing").unwrap();
    task.message_history_version = store.message_history_version("task-existing").unwrap();
    store.write_task(&task).unwrap();
    let native_updated_at = store
        .local_history_updated_at("task-existing")
        .unwrap()
        .parse::<u128>()
        .unwrap()
        + 6_000;
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: Some("Native title".to_string()),
            last_activity: Some(native_updated_at.to_string()),
            updated_at: Some(native_updated_at.to_string()),
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::AgentMessage {
            id: "native:fresh".to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![AgentMessagePart::Text {
                text: "Fresh native history.".to_string(),
            }],
            created_at: "2026-01-02T00:00:00.000Z".to_string(),
        }]),
        ..Default::default()
    });
    let (notifier, updates) = TaskUpdateNotifier::channel();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        notifier,
    )
    .unwrap();
    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        cursor: None,
    })
    .unwrap();
    while updates.try_recv().is_ok() {}

    let snapshot = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert_eq!(
        snapshot
            .task
            .title
            .as_ref()
            .map(|title| title.value.as_str()),
        Some("Existing")
    );
    assert!(matches!(
        snapshot.chat.items[0].parts.first(),
        Some(MessagePart::Text { text }) if text == "Stale cached history."
    ));
    let syncing = updates
        .recv_timeout(Duration::from_millis(250))
        .expect("stale cached history should start loading");
    assert!(matches!(
        syncing.kind,
        crate::task_events::TaskUpdateKind::HistorySync(TaskHistorySyncSnapshot::Syncing { .. })
    ));
    // Loading, attaching the permanent sink, and persisting replay happen in order
    // on the background worker; observe the final durable boundary.
    wait_until(|| {
        agent.loads.load(Ordering::SeqCst) == 1
            && agent.attaches.load(Ordering::SeqCst) == 1
            && store.read_messages("task-existing").is_ok_and(|messages| {
                matches!(
                    messages.as_slice(),
                    [message]
                        if matches!(
                            &message.chat.message,
                            NormalizedMessage::AgentMessage { id, .. }
                                if id == "native:fresh"
                        )
                )
            })
    });
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 0);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    let stored_messages = store.read_messages("task-existing").unwrap();
    assert_eq!(stored_messages.len(), 1);
    assert!(matches!(
        &stored_messages[0].chat.message,
        NormalizedMessage::AgentMessage {
            role: AgentMessageRole::Agent,
            parts,
            ..
        } if parts == &vec![AgentMessagePart::Text {
            text: "Fresh native history.".to_string(),
        }]
    ));
    let record = store.read_task("task-existing").unwrap();
    assert!(!record.unread);
    assert_eq!(record.last_activity, native_updated_at.to_string());
    assert_eq!(
        store.local_history_updated_at("task-existing").unwrap(),
        native_updated_at.to_string()
    );
    api.open_for_test(TaskOpenParams {
        task_id: "task-existing".into(),
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(25));
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
}

#[test]
fn open_returns_cached_task_while_native_session_refresh_is_blocked() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_list: AtomicBool::new(true),
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

    let (result_tx, result_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = api.open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        });
        let _ = result_tx.send(result);
    });

    let result = result_rx.recv_timeout(Duration::from_millis(250));
    agent.block_list.store(false, Ordering::SeqCst);
    let snapshot = result
        .expect("task/open waited for native session listing")
        .unwrap();
    assert_eq!(snapshot.task.task_id.as_str(), "task-existing");
}

#[test]
fn open_without_a_cached_native_catalog_recovers_the_native_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_config_catalog: Some(config_catalog("gpt-5.5")),
        resume_commands_catalog: Some(command_catalog()),
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: None,
            last_activity: Some("9999999999999".to_string()),
            updated_at: Some("9999999999999".to_string()),
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

    api.open_for_test(TaskOpenParams {
        task_id: "task-existing".into(),
    })
    .unwrap();
    wait_until(|| {
        agent.resumes.load(Ordering::SeqCst) == 1
            && agent.attaches.load(Ordering::SeqCst) == 1
            && store
                .read_task("task-existing")
                .is_ok_and(|task| task.config_options_catalog.is_some())
    });
    let recovered = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert!(agent.resumes.load(Ordering::SeqCst) >= 1);
    assert_eq!(agent.list_calls.load(Ordering::SeqCst), 0);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
    assert_eq!(recovered.agent_config.state, LiveSessionDataState::Ready);
    assert_eq!(
        protocol_value_id(&recovered.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
    assert_eq!(recovered.agent_commands.state, LiveSessionDataState::Ready);
}

#[test]
fn failed_native_session_listing_is_not_cached() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        fail_list: true,
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

    for expected in [2, 4] {
        assert!(api.refresh_native_session_catalogs().is_err());
        assert_eq!(agent.list_calls.load(Ordering::SeqCst), expected);
    }

    assert_eq!(agent.list_calls.load(Ordering::SeqCst), 4);
}

#[test]
fn send_does_not_wait_for_or_apply_a_blocked_history_listing() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.updated_at = "2026-01-01T00:00:00.000Z".to_string();
    task.last_activity = task.updated_at.clone();
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_list: AtomicBool::new(true),
        resume_after_restart_unavailable: true,
        loaded_session_id: Some("native-session".to_string()),
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: None,
            last_activity: Some("2026-01-02T00:00:00.000Z".to_string()),
            updated_at: Some("2026-01-02T00:00:00.000Z".to_string()),
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::AgentMessage {
            id: "native-history".to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![AgentMessagePart::Text {
                text: "History only the Agent had.".to_string(),
            }],
            created_at: "2026-01-02T00:00:00.000Z".to_string(),
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

    let refresh_api = api.clone();
    let refresh = std::thread::spawn(move || refresh_api.refresh_native_session_catalogs());
    wait_until(|| agent.list_calls.load(Ordering::SeqCst) == 1);
    api.send(send_params("task-existing", "What next?"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    let prompted_while_listing_blocked = agent.prompts.load(Ordering::SeqCst);
    agent.block_list.store(false, Ordering::SeqCst);
    refresh.join().unwrap().unwrap();

    assert_eq!(prompted_while_listing_blocked, 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.closes.load(Ordering::SeqCst), 0);
    let messages = store.read_messages("task-existing").unwrap();
    let texts = messages
        .iter()
        .filter_map(|message| match &message.chat.message {
            NormalizedMessage::AgentMessage {
                role: AgentMessageRole::Agent,
                parts,
                ..
            } => parts.iter().find_map(|part| match part {
                AgentMessagePart::Text { text } => Some(text.as_str()),
                _ => None,
            }),
            NormalizedMessage::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(texts, ["What next?"]);
}

#[test]
fn open_does_not_reload_native_history_for_an_acquired_prepared_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-draft", "/tmp/openaide-unit-workspace/app");
    task.lifecycle = test_new_task_lifecycle();
    task.agent_session_id = Some("prepared-session".to_string());
    task.updated_at = "2026-01-01T00:00:00.000Z".to_string();
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "prepared-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: None,
            last_activity: Some("2026-01-02T00:00:00.000Z".to_string()),
            updated_at: Some("2026-01-02T00:00:00.000Z".to_string()),
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
    api.create_for_test(TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    })
    .unwrap();
    wait_until(|| {
        matches!(
            store.read_task("task-draft").unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });
    let loads_after_acquire = agent.loads.load(Ordering::SeqCst);

    api.open_for_test(TaskOpenParams {
        task_id: "task-draft".into(),
    })
    .unwrap();

    assert_eq!(agent.loads.load(Ordering::SeqCst), loads_after_acquire);
}

#[test]
fn history_load_failure_adds_activity_and_leaves_task_sendable() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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
                message_type: "agent_message".to_string(),
                message_id: "cached_message".to_string(),
                message: NormalizedMessage::AgentMessage {
                    id: "cached:stale".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: "Stale cached history.".to_string(),
                    }],
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                },
            },
        )
        .unwrap();
    let mut task = store.read_task("task-existing").unwrap();
    task.message_history_version = store.message_history_version("task-existing").unwrap();
    store.write_task(&task).unwrap();
    let native_updated_at = store
        .local_history_updated_at("task-existing")
        .unwrap()
        .parse::<u128>()
        .unwrap()
        + 6_000;
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: Some("Native title".to_string()),
            last_activity: Some(native_updated_at.to_string()),
            updated_at: Some(native_updated_at.to_string()),
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::AgentMessage {
            id: "native:fresh".to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![AgentMessagePart::Text {
                text: "Fresh native history.".to_string(),
            }],
            created_at: "2026-01-02T00:00:00.000Z".to_string(),
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
    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        cursor: None,
    })
    .unwrap();

    let snapshot = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert!(matches!(
        snapshot.chat.items[0].parts.first(),
        Some(MessagePart::Text { text }) if text == "Stale cached history."
    ));
    wait_until(|| agent.loads.load(Ordering::SeqCst) == 1);
    std::thread::sleep(Duration::from_millis(25));
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.closes.load(Ordering::SeqCst), 0);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 0);
    let stored_messages = store.read_messages("task-existing").unwrap();
    assert!(matches!(
        &stored_messages[0].chat.message,
        NormalizedMessage::AgentMessage {
            role: AgentMessageRole::Agent,
            parts,
            ..
        } if parts == &vec![AgentMessagePart::Text {
            text: "Stale cached history.".to_string(),
        }]
    ));
    assert!(stored_messages.iter().any(|message| matches!(
        &message.chat.message,
        NormalizedMessage::Activity { title, status: ActivityStatus::Error, .. }
            if title == "History update failed"
    )));
    let current = api
        .project_task_snapshot(
            crate::tasks::snapshot::build_snapshot(&store, "task-existing", 100).unwrap(),
        )
        .unwrap();
    assert_eq!(
        current.send_capability.state,
        TaskSendCapabilityState::Ready
    );
    assert!(matches!(
        current.history_sync,
        TaskHistorySyncSnapshot::Idle { .. }
    ));
}

#[test]
fn open_resumes_native_session_when_cached_history_is_fresh() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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
                message_type: "agent_message".to_string(),
                message_id: "cached_message".to_string(),
                message: NormalizedMessage::AgentMessage {
                    id: "cached:current".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: "Current cached history.".to_string(),
                    }],
                    created_at: "2026-01-02T00:00:00.000Z".to_string(),
                },
            },
        )
        .unwrap();
    let mut task = store.read_task("task-existing").unwrap();
    task.message_history_version = store.message_history_version("task-existing").unwrap();
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_config_catalog: Some(config_catalog("gpt-5.5")),
        resume_commands_catalog: Some(command_catalog()),
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: Some("Older native title".to_string()),
            last_activity: Some("2026-01-01T00:00:00.000Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00.000Z".to_string()),
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::AgentMessage {
            id: "native:older".to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![AgentMessagePart::Text {
                text: "Older native history.".to_string(),
            }],
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
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
    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        cursor: None,
    })
    .unwrap();

    let snapshot = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    wait_until(|| {
        agent.resumes.load(Ordering::SeqCst) == 1 && agent.attaches.load(Ordering::SeqCst) == 1
    });

    assert!(matches!(
        snapshot.history_sync,
        TaskHistorySyncSnapshot::Idle { .. }
    ));
    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(
        snapshot
            .task
            .title
            .as_ref()
            .map(|title| title.value.as_str()),
        Some("Existing")
    );
    assert!(matches!(
        snapshot.chat.items[0].parts.first(),
        Some(MessagePart::Text { text }) if text == "Current cached history."
    ));
    let record = store.read_task("task-existing").unwrap();
    assert!(!record.unread);
    assert_eq!(record.last_activity, "2026-01-02T00:00:00.000Z");
}

#[test]
fn open_loads_native_session_when_history_is_unordered_and_resume_is_unsupported() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.updated_at = "2026-01-02T00:00:00.000Z".to_string();
    task.last_activity = "2026-01-02T00:00:00.000Z".to_string();
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "native-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: Some("Unordered native session".to_string()),
            last_activity: None,
            updated_at: None,
        }]),
        replayed_messages: Mutex::new(vec![NormalizedMessage::AgentMessage {
            id: "native:unordered".to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![AgentMessagePart::Text {
                text: "History with no ordering timestamp.".to_string(),
            }],
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
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
    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        cursor: None,
    })
    .unwrap();

    let snapshot = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert!(matches!(
        snapshot.history_sync,
        TaskHistorySyncSnapshot::Idle { .. }
    ));
    wait_until(|| {
        agent.resumes.load(Ordering::SeqCst) == 1
            && agent.loads.load(Ordering::SeqCst) == 1
            && agent.attaches.load(Ordering::SeqCst) == 1
            && store.read_messages("task-existing").is_ok_and(|messages| {
                matches!(
                    messages.as_slice(),
                    [message]
                        if matches!(
                            &message.chat.message,
                            NormalizedMessage::AgentMessage { id, .. }
                                if id == "native:unordered"
                        )
                )
            })
    });
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
}

#[test]
fn mark_read_clears_unread_without_refreshing_native_session_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.unread = true;
    task.attention = Some(crate::storage::records::TaskAttentionEvent::new(
        "attention-1",
        crate::storage::records::TaskAttentionReason::Finished,
        "2026-01-02T00:00:00Z",
    ));
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
        .mark_read_for_test(TaskMarkReadParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert!(!snapshot.task.unread);
    assert!(snapshot.task.attention.is_none());
    assert!(!store.read_task("task-existing").unwrap().unread);
    assert!(store
        .read_task("task-existing")
        .unwrap()
        .attention
        .is_none());
    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
}

#[test]
fn opening_second_finished_task_keeps_first_inactive_and_unread() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    for (task_id, attention_id) in [
        ("task-first", "attention-first"),
        ("task-second", "attention-second"),
    ] {
        let mut task = task_record(task_id, "/tmp/openaide-unit-workspace/app");
        task.status = TaskStatus::Inactive;
        task.unread = true;
        task.attention = Some(crate::storage::records::TaskAttentionEvent::new(
            attention_id,
            crate::storage::records::TaskAttentionReason::Finished,
            "2026-01-02T00:00:00Z",
        ));
        store.write_task(&task).unwrap();
    }
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let opened = api
        .open_for_test(TaskOpenParams {
            task_id: "task-second".into(),
        })
        .unwrap();

    assert!(!opened.task.unread);
    assert!(opened.task.attention.is_none());
    let first = store.read_task("task-first").unwrap();
    assert_eq!(first.status, TaskStatus::Inactive);
    assert!(first.unread);
    assert_eq!(
        first
            .attention
            .as_ref()
            .map(|event| event.event_id.as_str()),
        Some("attention-first")
    );
}

#[test]
fn unrelated_task_responses_preserve_current_history_sync_state() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.unread = true;
    store.write_task(&task).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let generation = api
        .history_sync
        .begin_passive("task-existing")
        .expect("history generation")
        .value();

    for expected in [
        TaskHistorySyncSnapshot::Syncing { generation },
        TaskHistorySyncSnapshot::Idle { generation },
        TaskHistorySyncSnapshot::Updated { generation },
    ] {
        api.publish_history_sync("task-existing", expected.clone());
        let mut task = store.read_task("task-existing").unwrap();
        task.unread = true;
        task.revision += 1;
        store.write_task(&task).unwrap();

        let snapshot = api
            .mark_read_for_test(TaskMarkReadParams {
                task_id: "task-existing".into(),
            })
            .unwrap();

        assert_eq!(snapshot.history_sync, expected);
    }
}

#[test]
fn first_send_accepts_starting_task_without_history_sync() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut new_task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    new_task.lifecycle = test_new_task_lifecycle();
    store.write_task(&new_task).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    api.create_for_test(TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    })
    .unwrap();
    wait_until(|| {
        matches!(
            store.read_task("task-existing").unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });

    let accepted = api.send(send_params("task-existing", "hello")).unwrap();

    assert_eq!(accepted.task.task.status, ProtocolTaskStatus::Starting);
    assert_eq!(
        accepted.task.history_sync,
        TaskHistorySyncSnapshot::Idle { generation: 0 }
    );
    let record = store.read_task("task-existing").unwrap();
    assert_eq!(record.lifecycle, TaskLifecycle::Visible);
    if matches!(record.status, TaskStatus::Starting | TaskStatus::Active) {
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
    let defaults = store.read_new_task_defaults().unwrap();
    assert_eq!(
        defaults.project_id,
        Some(project_id_for_workspace("/tmp/openaide-unit-workspace/app"))
    );
    assert_eq!(defaults.agent_id, Some(AgentId::from("codex")));
}

#[test]
fn send_returns_after_durable_acceptance_without_waiting_for_session_start() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
    let send_api = api.clone();

    std::thread::spawn(move || {
        accepted_tx
            .send(send_api.send(send_params("task-existing", "hello")))
            .unwrap();
    });

    wait_until(|| agent.starts.load(Ordering::SeqCst) == 1);
    let accepted = accepted_rx.recv_timeout(Duration::from_millis(100));
    let accepted = accepted
        .expect("Send should return before Native Session startup")
        .unwrap();
    let blockers = api.shutdown_blockers().unwrap();
    let second = api.send(send_params("task-existing", "second"));
    agent.block_start.store(false, Ordering::SeqCst);

    assert!(accepted.task.chat.items.len() >= 2);
    assert_eq!(blockers.active_turns, 1);
    assert_eq!(second.unwrap_err().code, ProtocolErrorCode::Conflict);
}

#[test]
fn accepted_task_becomes_running_only_when_agent_prompt_starts() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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

    let accepted = api.send(send_params("task-existing", "hello")).unwrap();
    assert_eq!(accepted.task.task.status, ProtocolTaskStatus::Starting);
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(
        store.read_task("task-existing").unwrap().status,
        TaskStatus::Active,
    );

    agent.release_prompt.store(true, Ordering::SeqCst);
}

#[test]
fn send_while_working_accepts_a_steering_message_without_replacing_primary_work() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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

    let primary = api
        .send(send_params("task-existing", "start work"))
        .unwrap();
    wait_until(|| store.read_task("task-existing").unwrap().status == TaskStatus::Active);

    let steering = api
        .send(send_params("task-existing", "also check tests"))
        .expect("working Task should accept steering");

    let task = store.read_task("task-existing").unwrap();
    assert_eq!(task.status, TaskStatus::Active);
    assert_eq!(
        task.active_turn_id.as_deref(),
        Some(primary.turn_id.as_str())
    );
    assert_eq!(steering.turn_id, primary.turn_id);
    let messages = store.read_messages("task-existing").unwrap();
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::User { ref text, .. } if text == "also check tests"
    )));
    wait_until(|| agent.steers.load(Ordering::SeqCst) == 1);
    assert_eq!(
        agent.steer_calls.lock().unwrap().as_slice(),
        &[(
            "recorded-session".to_string(),
            "also check tests".to_string()
        )]
    );

    agent.release_prompt.store(true, Ordering::SeqCst);
}

#[test]
fn send_starts_agent_session_and_prompts_after_commit() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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

    api.send(send_params("task-existing", "hello")).unwrap();

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
fn send_requests_native_catalog_refresh_after_prompt_start() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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

    api.send(send_params("task-existing", "hello")).unwrap();

    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    wait_until(|| agent.list_calls.load(Ordering::SeqCst) >= 2);
}

#[test]
fn send_recovers_stale_active_turn_and_starts_current_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
    let accepted = api.send(send_params("task-existing", "why stuck")).unwrap();

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
                status: ActivityStatus::Interrupted,
                ..
            } if id == "turn:turn-stale"
        )
    }));
    assert!(messages.iter().any(|message| {
        matches!(
            message.chat.message,
            NormalizedMessage::Interruption {
                reason: InterruptionReason::BackendUnavailable,
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
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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

    api.send(send_params("task-existing", "hello")).unwrap();

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
fn send_after_restart_hydrates_loaded_native_session_state_authoritatively() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("stored-session".to_string());
    task.config_options_catalog = Some(config_catalog("gpt-5"));
    task.agent_commands_catalog = Some(command_catalog());
    task.model_id = Some("gpt-5".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        loaded_session_id: Some("stored-session".to_string()),
        config_catalog: Some(config_catalog("gpt-5.5")),
        commands_catalog: Some(command_catalog()),
        suppress_commands_on_attach: true,
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
    api.send(send_params("task-existing", "hello")).unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    let snapshot = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();
    let stored = store.read_task("task-existing").unwrap();

    assert_eq!(snapshot.agent_config.state, LiveSessionDataState::Ready);
    assert_eq!(
        protocol_value_id(&snapshot.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
    assert_eq!(snapshot.agent_commands.state, LiveSessionDataState::Ready);
    assert_eq!(snapshot.agent_commands.commands[0].name, "web");
    assert_eq!(task_config_id(&stored, "model"), Some("gpt-5.5"));
    assert_eq!(stored.model_id.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        stored
            .config_options_catalog
            .as_ref()
            .and_then(|catalog| catalog.options.first())
            .and_then(|option| option.current_value.as_id()),
        Some("gpt-5.5")
    );
    assert_eq!(
        stored
            .agent_commands_catalog
            .as_ref()
            .and_then(|catalog| catalog.commands.first())
            .map(|command| command.name.as_str()),
        Some("web")
    );
}

#[test]
fn send_preserves_hydrated_session_state_when_resume_returns_identity_only() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
    let mut task = store.read_task("task-existing").unwrap();
    task.agent_session_id = Some("live-session".to_string());
    task.config_options_catalog = Some(config_catalog("gpt-5.5"));
    task.agent_commands_catalog = Some(command_catalog());
    task.model_id = Some("gpt-5.5".to_string());
    store.write_task(&task).unwrap();

    api.send(send_params("task-existing", "hello")).unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 0);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 0);
    let snapshot = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();
    let stored = store.read_task("task-existing").unwrap();

    assert_eq!(snapshot.agent_config.state, LiveSessionDataState::Ready);
    assert_eq!(
        protocol_value_id(&snapshot.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
    assert_eq!(snapshot.agent_commands.state, LiveSessionDataState::Ready);
    assert_eq!(snapshot.agent_commands.commands[0].name, "web");
    assert_eq!(task_config_id(&stored, "model"), Some("gpt-5.5"));
    assert_eq!(stored.model_id.as_deref(), Some("gpt-5.5"));
}

#[test]
fn send_after_restart_does_not_replace_session_when_stored_session_load_times_out() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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

    api.send(send_params("task-existing", "hello")).unwrap();

    wait_until(|| store.read_task("task-existing").unwrap().status == TaskStatus::Inactive);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 0);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.read_task("task-existing").unwrap().agent_session_id,
        Some("stored-session".to_string())
    );
}

#[test]
fn send_rejects_task_when_current_agent_registry_no_longer_has_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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

    let error = api.send(send_params("task-existing", "hello")).unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::CapabilityUnavailable);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    assert!(store.read_messages("task-existing").unwrap().is_empty());
}

#[test]
fn send_tolerates_attach_time_command_catalog_revision_bump() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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

    let accepted = api.send(send_params("task-existing", "hello")).unwrap();

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
fn send_start_failure_returns_task_to_idle() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let accepted = api
        .send(send_params("task-existing", "hello"))
        .expect("a durably committed send must remain accepted");

    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    let messages = store.read_messages("task-existing").unwrap();
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::User { ref text, .. } if text == "hello"
    )));
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Interrupted,
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
}

#[test]
fn send_session_attach_failure_returns_task_to_idle_and_closes_new_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
    let accepted = api
        .send(send_params("task-existing", "hello"))
        .expect("a durably committed send must remain accepted");

    wait_until(|| store.read_task("task-existing").unwrap().status == TaskStatus::Inactive);

    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(agent.closes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    let task = store.read_task("task-existing").unwrap();
    assert_eq!(task.status, TaskStatus::Inactive);
    assert_eq!(task.active_turn_id, None);
    assert_eq!(task.agent_session_id.as_deref(), Some("recorded-session"));
}

#[test]
fn send_post_commit_start_failure_allows_client_to_resend_inline_image() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
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
    let params = TaskSendParams {
        task_id: "task-existing".into(),
        message: ComposerMessage {
            text: Some("hello".to_string()),
            images: vec![inline_image()],
            attachments: Vec::new(),
        },
    };
    let accepted = api
        .send(params)
        .expect("a durably committed send must remain accepted");
    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    let retry = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            message: ComposerMessage {
                text: Some("reuse".to_string()),
                images: vec![inline_image()],
                attachments: Vec::new(),
            },
        })
        .expect("inline image bytes remain client-owned and can be resent");

    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    assert!(retry.turn_id.as_str().starts_with("turn_"));
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    let messages = store.read_messages("task-existing").unwrap();
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::User { ref text, .. } if text == "hello"
    )));
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Interrupted,
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
}

#[test]
fn send_accepts_a_file_only_message_and_forwards_the_original_path() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let selected = workspace.join("large-model.bin");
    std::fs::write(&selected, b"model bytes").unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            workspace.to_string_lossy().as_ref(),
        ))
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
    let task_id = openaide_app_server_protocol::ids::TaskId::from("task-existing");
    let attachment = api
        .attachment_runtime()
        .create_local_file_reference(
            crate::attachment_runtime::AttachmentOwner::new(
                &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
                &task_id,
            ),
            &selected,
            None,
        )
        .unwrap();

    api.send(TaskSendParams {
        task_id,
        message: ComposerMessage {
            text: None,
            images: Vec::new(),
            attachments: vec![attachment.handle_id],
        },
    })
    .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let prompts = agent.prompt_attachments.lock().unwrap();
    assert_eq!(prompts[0][0].kind, "file_reference");
    assert_eq!(
        prompts[0][0].path.as_deref(),
        Some(selected.to_string_lossy().as_ref())
    );
    let messages = store.read_messages("task-existing").unwrap();
    assert!(messages.iter().any(|message| matches!(
        &message.chat.message,
        NormalizedMessage::User { attachments, .. }
            if attachments.first().and_then(|attachment| attachment.path.as_deref())
                == Some(selected.to_string_lossy().as_ref())
    )));
    let user_message = messages
        .iter()
        .find(|message| matches!(message.chat.message, NormalizedMessage::User { .. }))
        .unwrap();
    let resolved = api
        .resolve_sent_file(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            &TaskId::from("task-existing"),
            &user_message.chat.message_id,
            0,
        )
        .unwrap();
    assert_eq!(resolved.path, selected);
    assert_eq!(resolved.label, "large-model.bin");
}

#[test]
fn send_start_failure_does_not_poison_later_task_start() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-first",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    store
        .write_task(&task_record(
            "task-second",
            "/tmp/openaide-unit-workspace/app",
        ))
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
        .send(send_params("task-first", "first"))
        .expect("a durably committed send must remain accepted");

    assert!(accepted.turn_id.as_str().starts_with("turn_"));
    wait_until(|| {
        store
            .read_task("task-first")
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    let first = store.read_task("task-first").unwrap();
    assert_eq!(first.status, TaskStatus::Inactive);
    assert_eq!(first.active_turn_id, None);
    assert_eq!(first.agent_session_id, None);

    api.send(send_params("task-second", "second")).unwrap();
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
fn send_trims_surrounding_whitespace_from_prompt_text() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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

    let accepted = api
        .send(send_params(
            "task-existing",
            "  first line\n  indented line  ",
        ))
        .unwrap();

    assert_eq!(
        accepted.task.chat.items[0].parts[0],
        openaide_app_server_protocol::snapshot::MessagePart::Text {
            text: "first line\n  indented line".to_string()
        }
    );
}

#[test]
fn send_rejects_a_second_prompt_while_active_turn_is_blocked_on_permission() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
        .send(send_params("task-existing", "start work"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let mut blocked = store.read_task("task-existing").unwrap();
    blocked.status = TaskStatus::Waiting;
    store.write_task(&blocked).unwrap();
    let error = api
        .send(send_params("task-existing", "why no answer?"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 1);
    let record = store.read_task("task-existing").unwrap();
    assert_eq!(record.status, TaskStatus::Waiting);
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

    api.cancel_for_test(TaskCancelParams {
        task_id: "task-existing".into(),
        turn_id: Some(first.turn_id),
    })
    .unwrap();
}

#[test]
fn send_accepts_the_current_task_after_an_unrelated_revision_change() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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

    let mut current = store.read_task("task-existing").unwrap();
    current.title = crate::storage::records::TaskTitle::new(
        "Updated elsewhere",
        crate::storage::records::TaskTitleSource::User,
    );
    store.write_task(&current).unwrap();

    let accepted = api.send(send_params("task-existing", "hello")).unwrap();

    assert_eq!(accepted.task.chat.items.len(), 2);
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
fn send_keeps_committed_message_when_config_changes_while_agent_session_opens() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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

    let accepted = api.send(send_params("task-existing", "hello")).unwrap();

    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task_config_id(&task, "model") == Some("new-model"))
            .unwrap_or(false)
    });

    let record = store.read_task("task-existing").unwrap();
    assert_eq!(task_config_id(&record, "model"), Some("new-model"));
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
fn send_rejects_invalid_inline_image_without_committing() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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

    let mut image = inline_image();
    image.data = "not-base64".to_string();
    let error = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            message: ComposerMessage {
                text: Some("hello".to_string()),
                images: vec![image],
                attachments: Vec::new(),
            },
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::ValidationFailed);
    assert_eq!(error.message, "Image data is invalid");
    assert!(store.read_messages("task-existing").unwrap().is_empty());
    assert_eq!(
        store.read_task("task-existing").unwrap().active_turn_id,
        None
    );
}

#[test]
fn send_commits_inline_image_as_image_chat_content() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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
    let accepted = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            message: ComposerMessage {
                text: Some("hello".to_string()),
                images: vec![inline_image()],
                attachments: Vec::new(),
            },
        })
        .unwrap();

    assert_eq!(accepted.task.chat.items[0].parts.len(), 2);
    let MessagePart::Image {
        media_type,
        data_url,
        uri,
    } = &accepted.task.chat.items[0].parts[1]
    else {
        panic!("expected Image part");
    };
    assert_eq!(media_type, "image/png");
    assert_eq!(data_url, "data:image/png;base64,aW1hZ2U=");
    assert_eq!(uri, &None);
    wait_until(|| store.read_task("task-existing").unwrap().status == TaskStatus::Inactive);
    assert!(
        store
            .read_task("task-existing")
            .unwrap()
            .supports_image_input,
        "sending an image must not remove image capability from the Task"
    );
}

#[test]
fn send_commits_inline_image_without_an_empty_text_part() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.title = None;
    task.lifecycle = test_new_task_lifecycle();
    store.write_task(&task).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let acquired = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .unwrap();
    assert_eq!(acquired.task.task_id.as_str(), "task-existing");
    wait_until(|| {
        matches!(
            store.read_task("task-existing").unwrap().preparation,
            TaskPreparationRecord::Ready
        )
    });
    let mut image_capable = store.read_task("task-existing").unwrap();
    image_capable.supports_image_input = true;
    store.write_task(&image_capable).unwrap();

    let accepted = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            message: ComposerMessage {
                text: None,
                images: vec![inline_image()],
                attachments: Vec::new(),
            },
        })
        .unwrap();

    assert_eq!(accepted.task.chat.items[0].parts.len(), 1);
    let MessagePart::Image {
        media_type,
        data_url,
        uri,
    } = &accepted.task.chat.items[0].parts[0]
    else {
        panic!("expected inline Image message part");
    };
    assert_eq!(media_type, "image/png");
    assert_eq!(data_url, "data:image/png;base64,aW1hZ2U=");
    assert_eq!(uri, &None);
    assert_eq!(accepted.task.task.title, None);
    assert_eq!(store.read_task("task-existing").unwrap().title, None);
}

#[test]
fn rejected_send_keeps_inline_image_available_for_retry() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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
    let message = ComposerMessage {
        text: Some("hello".to_string()),
        images: vec![inline_image()],
        attachments: Vec::new(),
    };

    let mut unavailable = store.read_task("task-existing").unwrap();
    unavailable.agent_id = "missing-agent".to_string();
    store.write_task(&unavailable).unwrap();
    let error = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            message: message.clone(),
        })
        .unwrap_err();
    let mut available = store.read_task("task-existing").unwrap();
    available.agent_id = "codex".to_string();
    store.write_task(&available).unwrap();
    let accepted = api
        .send(TaskSendParams {
            task_id: "task-existing".into(),
            message,
        })
        .unwrap();

    assert_eq!(error.code, ProtocolErrorCode::CapabilityUnavailable);
    assert_eq!(accepted.task.chat.items[0].parts.len(), 2);
}

#[cfg(unix)]
#[test]
#[cfg(any())]
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
            message: ComposerMessage {
                text: Some("hello".to_string()),
                images: vec![inline_image()],
                attachments: Vec::new(),
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
fn cancel_stays_stopping_until_the_agent_prompt_settles() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_prompt: true,
        hold_cancelled_prompt: AtomicBool::new(true),
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

    api.mutations
        .commit_existing_task(
            "task-existing",
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                ctx.append_message(NormalizedMessage::Activity {
                    id: "turn:turn-active".to_string(),
                    title: "Working".to_string(),
                    status: ActivityStatus::Running,
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                    collapsed: true,
                    steps: Vec::new(),
                })?;
                ctx.append_message(NormalizedMessage::AgentMessage {
                    id: "agent-stream".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: "partial response".to_string(),
                    }],
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                })?;
                let task = ctx.task_mut();
                task.status = TaskStatus::Active;
                task.active_turn_id = Some("turn-active".to_string());
                Ok(TaskMutationResult::Changed)
            },
        )
        .unwrap();

    // Register a real live prompt so Stop must wait for its terminal response.
    let session = AgentSession::new("codex", "cancel-session");
    api.turn_runner.spawn_agent_turn(
        "task-existing".to_string(),
        "hello".to_string(),
        Vec::new(),
        "turn-active".to_string(),
        session,
        Arc::new(crate::tasks::turn_events::TaskSessionEventSink::new(
            api.mutations.clone(),
            "task-existing".to_string(),
            "cancel-session".to_string(),
            api.server_requests.clone(),
        )),
    );
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let snapshot = api
        .cancel_for_test(TaskCancelParams {
            task_id: "task-existing".into(),
            turn_id: Some("turn-active".into()),
        })
        .unwrap();

    let record = store.read_task("task-existing").unwrap();
    assert_eq!(record.status, TaskStatus::Stopping);
    assert_eq!(record.active_turn_id.as_deref(), Some("turn-active"));
    assert!(store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .any(|message| matches!(
            &message.chat.message,
            NormalizedMessage::AgentMessage {
                role: AgentMessageRole::Agent,
                parts,
                ..
            } if parts == &vec![AgentMessagePart::Text {
                text: "partial response".to_string(),
            }]
        )));
    assert_eq!(
        snapshot.task.status,
        openaide_app_server_protocol::snapshot::TaskStatus::Stopping
    );
    assert!(!store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .any(|message| matches!(message.chat.message, NormalizedMessage::Interruption { .. })));

    agent.release_cancelled_prompt.store(true, Ordering::SeqCst);
    wait_until(|| store.read_task("task-existing").unwrap().status == TaskStatus::Inactive);
    let finished = store.read_task("task-existing").unwrap();
    assert_eq!(finished.active_turn_id, None);
    let messages = store.read_messages("task-existing").unwrap();
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::Activity {
            status: ActivityStatus::Interrupted,
            ..
        }
    )));
    assert!(messages.iter().any(|message| matches!(
        message.chat.message,
        NormalizedMessage::Interruption {
            reason: InterruptionReason::Canceled,
            ..
        }
    )));
}

#[test]
fn cancel_timeout_closes_live_session_but_preserves_task_binding_for_resume() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
        .unwrap();
    let agent = Arc::new(RecordingAgent {
        block_prompt: true,
        hold_cancelled_prompt: AtomicBool::new(true),
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
    api.turn_runner
        .set_cancel_grace_period_for_test(Duration::from_millis(20));

    let sent = api.send(send_params("task-existing", "hello")).unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    api.cancel_for_test(TaskCancelParams {
        task_id: "task-existing".into(),
        turn_id: Some(sent.turn_id),
    })
    .unwrap();

    wait_until(|| {
        store.read_task("task-existing").unwrap().status == TaskStatus::Inactive
            && agent.closes.load(Ordering::SeqCst) == 1
    });
    let task = store.read_task("task-existing").unwrap();
    assert_eq!(task.active_turn_id, None);
    assert_eq!(task.agent_session_id.as_deref(), Some("recorded-session"));
    assert_eq!(agent.closes.load(Ordering::SeqCst), 1);
    assert!(store
        .read_messages("task-existing")
        .unwrap()
        .iter()
        .any(|message| matches!(
            message.chat.message,
            NormalizedMessage::Activity {
                status: ActivityStatus::Interrupted,
                ..
            }
        )));

    agent.release_cancelled_prompt.store(true, Ordering::SeqCst);
    wait_until(|| agent.prompt_completions.load(Ordering::SeqCst) == 1);
    let task_after_late_prompt = store.read_task("task-existing").unwrap();
    assert_eq!(task_after_late_prompt.status, TaskStatus::Inactive);
    assert_eq!(task_after_late_prompt.active_turn_id, None);
    assert_eq!(
        task_after_late_prompt.agent_session_id.as_deref(),
        Some("recorded-session")
    );

    agent.release_prompt.store(true, Ordering::SeqCst);
    api.send(send_params("task-existing", "continue")).unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 2);
    assert_eq!(agent.starts.load(Ordering::SeqCst), 1);
    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(
        agent.prompt_calls.lock().unwrap().last(),
        Some(&("recorded-session".to_string(), "continue".to_string()))
    );
}

#[test]
fn cancel_signals_live_agent_turn_started_by_task_send() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
    let sent = api.send(send_params("task-existing", "hello")).unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    api.cancel_for_test(TaskCancelParams {
        task_id: "task-existing".into(),
        turn_id: Some(sent.turn_id),
    })
    .unwrap();

    wait_until(|| agent.cancels.load(Ordering::SeqCst) == 1);
    assert_eq!(agent.cancels.load(Ordering::SeqCst), 1);
}

#[test]
fn stale_cancel_cannot_retire_a_newer_accepted_send() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
    let first = api.send(send_params("task-existing", "first")).unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    let (stale_read_tx, stale_read_rx) = std::sync::mpsc::sync_channel(0);
    let (release_read_tx, release_read_rx) = std::sync::mpsc::sync_channel(0);
    store.after_next_task_read_for_test(move || {
        stale_read_tx.send(()).unwrap();
        release_read_rx.recv().unwrap();
    });
    let cancel_api = api.clone();
    let cancel = std::thread::spawn(move || {
        cancel_api.cancel_for_test(TaskCancelParams {
            task_id: "task-existing".into(),
            turn_id: Some(first.turn_id),
        })
    });
    stale_read_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("Cancel should read the active Turn");

    agent.release_prompt.store(true, Ordering::SeqCst);
    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.active_turn_id.is_none())
            .unwrap_or(false)
    });
    agent.release_prompt.store(false, Ordering::SeqCst);
    // Cancel is paused by the read hook before it enters Task serialization, so
    // the second Send can deterministically replace the active prompt first.
    let second = api.send(send_params("task-existing", "second")).unwrap();

    release_read_tx.send(()).unwrap();
    let stale_cancel = cancel.join().unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 2);

    assert_eq!(stale_cancel.unwrap_err().code, ProtocolErrorCode::Conflict);
    assert_eq!(
        agent.prompt_calls.lock().unwrap().last().cloned(),
        Some(("recorded-session".to_string(), "second".to_string()))
    );
    assert!(second.turn_id.as_str().starts_with("turn_"));

    agent.release_prompt.store(true, Ordering::SeqCst);
    wait_until(|| {
        store
            .read_task("task-existing")
            .map(|task| task.active_turn_id.is_none())
            .unwrap_or(false)
    });
}

#[test]
fn failed_cancel_commit_does_not_retire_an_accepted_send() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut project_anchor = task_record("task-project-anchor", "/tmp/openaide-unit-workspace/app");
    project_anchor.lifecycle = TaskLifecycle::Visible;
    store.write_task(&project_anchor).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_start: AtomicBool::new(true),
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
    let draft = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .unwrap();
    wait_until(|| agent.starts.load(Ordering::SeqCst) == 1);
    agent.block_start.store(false, Ordering::SeqCst);
    wait_until(|| {
        matches!(
            store
                .read_task(draft.task.task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });
    let accepted = api
        .send(send_params(draft.task.task_id.as_str(), "hello"))
        .unwrap();
    wait_until(|| agent.prompts.load(Ordering::SeqCst) == 1);

    store.fail_next_task_write_for_test();
    let error = api
        .cancel_for_test(TaskCancelParams {
            task_id: draft.task.task_id.clone(),
            turn_id: Some(accepted.turn_id.clone()),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Internal);
    assert_eq!(
        store
            .read_task(draft.task.task_id.as_str())
            .unwrap()
            .active_turn_id
            .as_deref(),
        Some(accepted.turn_id.as_str())
    );
    assert_eq!(api.shutdown_blockers().unwrap().active_turns, 1);

    api.cancel_for_test(TaskCancelParams {
        task_id: draft.task.task_id,
        turn_id: Some(accepted.turn_id),
    })
    .unwrap();
}

#[test]
fn support_recovery_clears_live_stuck_turn_without_waiting_for_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
        ))
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
    api.send(send_params("task-existing", "hello")).unwrap();
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
fn support_recovery_retires_an_accepted_turn_still_starting_its_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_resume: AtomicBool::new(true),
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
    let accepted = api.send(send_params("task-existing", "hello")).unwrap();
    wait_until(|| agent.resumes.load(Ordering::SeqCst) == 1);

    let result = api
        .recover_stuck_sessions(SupportRecoverStuckSessionsParams {})
        .unwrap();
    let blockers = api.shutdown_blockers().unwrap();
    agent.block_resume.store(false, Ordering::SeqCst);

    assert_eq!(result.recovered_tasks.len(), 1);
    assert_eq!(blockers.active_turns, 0);
    assert_eq!(
        store.read_task("task-existing").unwrap().active_turn_id,
        None
    );
    assert_eq!(agent.prompts.load(Ordering::SeqCst), 0);
    assert_eq!(
        result.recovered_tasks[0].task.task_id.as_str(),
        accepted.task.task.task_id.as_str()
    );
}

#[test]
fn cancel_rejects_mismatched_turn_id() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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
    api.send(send_params("task-existing", "hello")).unwrap();

    let error = api
        .cancel_for_test(TaskCancelParams {
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
fn set_config_option_rejects_a_task_without_a_native_session_or_live_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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

    let error = api
        .set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: protocol_config_id("gpt-5.5"),
            client_mutation_id: "mutation-1".into(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Internal);
    assert!(store
        .read_task("task-existing")
        .unwrap()
        .config_options_catalog
        .is_none());
}

#[test]
fn set_config_option_recovers_an_inactive_native_session_before_agent_io() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_config_catalog: Some(config_catalog("gpt-5")),
        config_requires_active_session: true,
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

    let snapshot = api
        .set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: protocol_config_id("gpt-5.5"),
            client_mutation_id: "mutation-after-idle-close".into(),
        })
        .unwrap();

    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
    assert_eq!(
        protocol_value_id(&snapshot.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
}

#[test]
fn config_recovery_loads_when_the_agent_does_not_support_resume() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    task.agent_session_id = Some("native-session".to_string());
    task.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&task).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_after_restart_unavailable: true,
        config_catalog: Some(config_catalog("gpt-5")),
        config_requires_active_session: true,
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

    api.set_config_option_for_test(TaskSetConfigOptionParams {
        task_id: "task-existing".into(),
        config_id: "model".into(),
        value: protocol_config_id("gpt-5.5"),
        client_mutation_id: "mutation-load-fallback".into(),
    })
    .unwrap();

    assert_eq!(agent.resumes.load(Ordering::SeqCst), 1);
    assert_eq!(agent.loads.load(Ordering::SeqCst), 1);
    assert_eq!(agent.attaches.load(Ordering::SeqCst), 1);
}

#[test]
fn restart_hides_persisted_agent_controls_until_native_session_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("persisted-session".to_string());
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    record.agent_commands_catalog = Some(command_catalog());
    store.write_task(&record).unwrap();

    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    let snapshot = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert_eq!(
        snapshot.agent_config.state,
        LiveSessionDataState::Unavailable
    );
    assert!(snapshot.agent_config.options.is_empty());
    assert_eq!(
        snapshot.agent_commands.state,
        LiveSessionDataState::Unavailable
    );
    assert!(snapshot.agent_commands.commands.is_empty());
}

#[test]
fn restart_clears_a_config_mutation_interrupted_during_agent_io() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("persisted-session".to_string());
    crate::tasks::config_options::begin_task_config_mutation(
        &mut record,
        "mutation-interrupted".to_string(),
        "model".to_string(),
        ConfigOptionCurrentValue::id("gpt-5.5"),
    )
    .unwrap();
    store.write_task(&record).unwrap();

    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let snapshot = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();
    let recovered = store.read_task("task-existing").unwrap();

    assert_eq!(snapshot.agent_config.pending_change, None);
    assert_eq!(recovered.config_mutation.pending, None);
    assert_eq!(recovered.config_mutation.sequence, 1);
}

#[test]
fn task_open_republishes_controls_from_recovered_native_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("persisted-session".to_string());
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    record.agent_commands_catalog = Some(command_catalog());
    record.updated_at = "2025-12-31T00:00:00.000Z".to_string();
    record.last_activity = record.updated_at.clone();
    store.write_task(&record).unwrap();
    store
        .append_message(
            "task-existing",
            ChatMessage {
                cursor: "m:1".to_string(),
                identity: "cached:controls".to_string(),
                message_type: "agent_message".to_string(),
                message_id: "cached-controls".to_string(),
                message: NormalizedMessage::AgentMessage {
                    id: "cached:controls".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: "Cached history".to_string(),
                    }],
                    created_at: "2025-12-31T00:00:00.000Z".to_string(),
                },
            },
        )
        .unwrap();
    let mut record = store.read_task("task-existing").unwrap();
    record.message_history_version = store.message_history_version("task-existing").unwrap();
    store.write_task(&record).unwrap();
    let native_updated_at = store
        .local_history_updated_at("task-existing")
        .unwrap()
        .parse::<u128>()
        .unwrap()
        + 6_000;
    let agent = Arc::new(RecordingAgent {
        config_catalog: Some(config_catalog("gpt-5.5")),
        commands_catalog: Some(command_catalog()),
        loaded_session_id: Some("persisted-session".to_string()),
        resume_after_restart_unavailable: true,
        listed_sessions: Mutex::new(vec![AgentListedSession {
            session_id: "persisted-session".to_string(),
            cwd: "/tmp/openaide-unit-workspace/app".to_string(),
            title: None,
            last_activity: Some(native_updated_at.to_string()),
            updated_at: Some(native_updated_at.to_string()),
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
    api.refresh_native_session_catalogs().unwrap();

    let initial = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();
    assert_eq!(
        initial.agent_config.state,
        LiveSessionDataState::Unavailable
    );
    assert_eq!(
        initial.agent_commands.state,
        LiveSessionDataState::Unavailable
    );

    wait_until(|| agent.loads.load(Ordering::SeqCst) == 1);
    let recovered = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();

    assert_eq!(recovered.agent_config.state, LiveSessionDataState::Ready);
    assert_eq!(
        protocol_value_id(&recovered.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
    assert_eq!(recovered.agent_commands.state, LiveSessionDataState::Ready);
    assert_eq!(recovered.agent_commands.commands[0].name, "web");
}

#[test]
fn set_config_option_without_native_session_does_not_project_persisted_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    let catalog = config_catalog("gpt-5");
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

    let error = api
        .set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: protocol_config_id("gpt-5.5"),
            client_mutation_id: "mutation-1".into(),
        })
        .unwrap_err();

    let stored = store.read_task("task-existing").unwrap();
    assert!(stored.config_options_catalog.is_none());
    assert_eq!(error.code, ProtocolErrorCode::Internal);
}

#[test]
fn set_config_option_without_native_session_does_not_project_unsupported_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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

    let error = api
        .set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "custom".into(),
            value: protocol_config_id("enabled"),
            client_mutation_id: "mutation-1".into(),
        })
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Internal);
}

#[test]
fn set_config_option_applies_to_running_task_live_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();

    let snapshot = api
        .set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: protocol_config_id("gpt-5.5"),
            client_mutation_id: "mutation-1".into(),
        })
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
    assert_eq!(task_config_id(&stored, "model"), Some("gpt-5.5"));
    assert_eq!(stored.model_id.as_deref(), Some("gpt-5.5"));
    assert_ne!(stored.updated_at, "2026-01-01T00:00:00.000Z");
    assert_eq!(stored.last_activity, "2026-01-01T00:00:00.000Z");
    assert_eq!(
        protocol_value_id(&snapshot.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
}

#[test]
fn set_config_option_applies_boolean_to_the_same_native_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.config_options_catalog = Some(boolean_config_catalog(false));
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
    record.agent_session_id = Some("session-existing".to_string());
    record.config_options_catalog = Some(boolean_config_catalog(false));
    store.write_task(&record).unwrap();

    let snapshot = api
        .set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "brave_mode".into(),
            value: AgentConfigOptionCurrentValue::Boolean { value: true },
            client_mutation_id: "mutation-boolean".into(),
        })
        .unwrap();

    assert_eq!(
        agent
            .typed_session_config_updates
            .lock()
            .unwrap()
            .as_slice(),
        [(
            "session-existing".to_string(),
            "brave_mode".to_string(),
            ConfigOptionCurrentValue::boolean(true),
        )]
    );
    let stored = store.read_task("task-existing").unwrap();
    assert_eq!(stored.agent_session_id.as_deref(), Some("session-existing"));
    assert_eq!(
        snapshot.agent_config.options[0].current_value,
        AgentConfigOptionCurrentValue::Boolean { value: true },
    );
}

#[test]
fn set_config_option_applies_to_prepared_task_native_session() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-prepared", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("session-prepared".to_string());
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
    set_live_config_catalog(&store, "task-prepared", config_catalog("gpt-5"));

    api.set_config_option_for_test(TaskSetConfigOptionParams {
        task_id: "task-prepared".into(),
        config_id: "model".into(),
        value: protocol_config_id("gpt-5.5"),
        client_mutation_id: "mutation-prepared".into(),
    })
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
fn set_config_option_projects_the_pending_client_mutation_during_agent_io() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("session-a".to_string());
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_set_config: AtomicBool::new(true),
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
    set_live_config_catalog(&store, "task-existing", config_catalog("gpt-5"));
    let setting_api = api.clone();
    let (result_tx, result_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        result_tx
            .send(
                setting_api.set_config_option_for_test(TaskSetConfigOptionParams {
                    task_id: "task-existing".into(),
                    config_id: "model".into(),
                    value: protocol_config_id("gpt-5.5"),
                    client_mutation_id: "mutation-pending".into(),
                }),
            )
            .unwrap();
    });
    wait_until(|| agent.session_config_updates.lock().unwrap().len() == 1);

    let pending = api
        .open_for_test(TaskOpenParams {
            task_id: "task-existing".into(),
        })
        .unwrap();
    agent.block_set_config.store(false, Ordering::SeqCst);
    let settled = result_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("the config mutation should settle")
        .unwrap();

    let pending_change = pending
        .agent_config
        .pending_change
        .expect("Agent I/O should expose the originating client mutation");
    assert_eq!(
        pending_change.client_mutation_id.as_str(),
        "mutation-pending"
    );
    assert_eq!(pending_change.config_id.as_str(), "model");
    assert_eq!(
        protocol_value_id(&pending_change.requested_value),
        Some("gpt-5.5")
    );
    assert_eq!(settled.agent_config.pending_change, None);
}

#[test]
fn same_task_config_changes_reach_agent_and_storage_in_admission_order() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("session-a".to_string());
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let agent = Arc::new(OrderedConfigAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    set_live_config_catalog(&store, "task-existing", ordered_config_catalog("gpt-5"));
    let older_api = api.clone();
    let older = std::thread::spawn(move || {
        older_api.set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: protocol_config_id("gpt-5.1"),
            client_mutation_id: "mutation-older".into(),
        })
    });
    wait_until(|| agent.first_request_started.load(Ordering::SeqCst));

    let newer_api = api.clone();
    let (submitted_tx, submitted_rx) = std::sync::mpsc::channel();
    let newer = std::thread::spawn(move || {
        submitted_tx.send(()).unwrap();
        newer_api.set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-existing".into(),
            config_id: "model".into(),
            value: protocol_config_id("gpt-5.2"),
            client_mutation_id: "mutation-newer".into(),
        })
    });
    submitted_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("newer config request should be submitted");
    let observation_deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < observation_deadline && agent.started_values.lock().unwrap().len() == 1 {
        std::thread::yield_now();
    }
    let newer_reached_agent_before_first_completed = agent.started_values.lock().unwrap().len() > 1;

    agent.release_first.store(true, Ordering::SeqCst);
    let older = older.join().unwrap().unwrap();
    let newer = newer.join().unwrap().unwrap();

    let stored = store.read_task("task-existing").unwrap();
    assert!(
        !newer_reached_agent_before_first_completed,
        "a newer config change must not overtake an admitted change for the same Task"
    );
    assert_eq!(
        protocol_value_id(&older.agent_config.options[0].current_value),
        Some("gpt-5.1")
    );
    assert_eq!(
        protocol_value_id(&newer.agent_config.options[0].current_value),
        Some("gpt-5.2")
    );
    assert_eq!(stored.model_id.as_deref(), Some("gpt-5.2"));
    assert_eq!(task_config_id(&stored, "model"), Some("gpt-5.2"));
    assert_eq!(stored.config_mutation.sequence, 2);
    assert_eq!(stored.config_mutation.pending, None);
    assert_eq!(
        agent.started_values.lock().unwrap().as_slice(),
        ["gpt-5.1".to_string(), "gpt-5.2".to_string()]
    );
    assert_eq!(
        agent.completed_values.lock().unwrap().as_slice(),
        ["gpt-5.1".to_string(), "gpt-5.2".to_string()]
    );
}

#[test]
fn blocked_config_change_does_not_stall_an_unrelated_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut blocked_task = task_record("task-blocked", "/tmp/openaide-unit-workspace/blocked");
    blocked_task.agent_session_id = Some("session-blocked".to_string());
    blocked_task.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&blocked_task).unwrap();
    let mut unrelated_task =
        task_record("task-unrelated", "/tmp/openaide-unit-workspace/unrelated");
    unrelated_task.agent_session_id = Some("session-unrelated".to_string());
    unrelated_task.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&unrelated_task).unwrap();
    let agent = Arc::new(OrderedConfigAgent::default());
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent.clone(),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    set_live_config_catalog(&store, "task-blocked", ordered_config_catalog("gpt-5"));
    set_live_config_catalog(&store, "task-unrelated", ordered_config_catalog("gpt-5"));

    let blocked_api = api.clone();
    let blocked = std::thread::spawn(move || {
        blocked_api.set_config_option_for_test(TaskSetConfigOptionParams {
            task_id: "task-blocked".into(),
            config_id: "model".into(),
            value: protocol_config_id("gpt-5.1"),
            client_mutation_id: "mutation-blocked".into(),
        })
    });
    wait_until(|| agent.first_request_started.load(Ordering::SeqCst));

    let unrelated_api = api.clone();
    let (submitted_tx, submitted_rx) = std::sync::mpsc::channel();
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    let unrelated = std::thread::spawn(move || {
        submitted_tx.send(()).unwrap();
        result_tx
            .send(
                unrelated_api.set_config_option_for_test(TaskSetConfigOptionParams {
                    task_id: "task-unrelated".into(),
                    config_id: "model".into(),
                    value: protocol_config_id("gpt-5.2"),
                    client_mutation_id: "mutation-unrelated".into(),
                }),
            )
            .unwrap();
    });
    submitted_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("unrelated config request should be submitted");
    let unrelated_result = result_rx.recv_timeout(Duration::from_secs(1));

    agent.release_first.store(true, Ordering::SeqCst);
    blocked.join().unwrap().unwrap();
    unrelated.join().unwrap();
    let unrelated_snapshot = unrelated_result
        .expect("an unrelated Task must complete while the first Task is blocked")
        .unwrap();

    assert_eq!(
        protocol_value_id(&unrelated_snapshot.agent_config.options[0].current_value),
        Some("gpt-5.2")
    );
    assert_eq!(
        task_config_id(&store.read_task("task-unrelated").unwrap(), "model"),
        Some("gpt-5.2")
    );
    assert_eq!(
        agent.completed_values.lock().unwrap().as_slice(),
        ["gpt-5.2".to_string(), "gpt-5.1".to_string()]
    );
}

#[test]
fn set_config_option_continues_after_concurrent_session_replacement_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("session-a".to_string());
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_set_config: AtomicBool::new(true),
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
    set_live_config_catalog(&store, "task-existing", config_catalog("gpt-5"));
    let setting_api = api.clone();
    let (result_tx, result_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        result_tx
            .send(
                setting_api.set_config_option_for_test(TaskSetConfigOptionParams {
                    task_id: "task-existing".into(),
                    config_id: "model".into(),
                    value: protocol_config_id("gpt-5.5"),
                    client_mutation_id: "session-a-change".into(),
                }),
            )
            .unwrap();
    });
    wait_until(|| agent.session_config_updates.lock().unwrap().len() == 1);
    let replacement_error = api
        .mutations
        .commit_existing_task("task-existing", TaskCommitOptions::metadata(), |ctx| {
            let task = ctx.task_mut();
            task.agent_session_id = Some("session-b".to_string());
            task.config_options_catalog = Some(config_catalog("gpt-5"));
            Ok(crate::tasks::mutation::TaskMutationResult::Changed)
        })
        .unwrap_err();
    assert!(
        matches!(replacement_error, RuntimeError::Internal(message) if
        message == "task mutation changed bound Native Session identity")
    );
    agent.block_set_config.store(false, Ordering::SeqCst);

    let snapshot = result_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("the stale session request should finish")
        .unwrap();
    let stored = store.read_task("task-existing").unwrap();

    assert_eq!(stored.agent_session_id.as_deref(), Some("session-a"));
    assert_eq!(task_config_id(&stored, "model"), Some("gpt-5.5"));
    assert_eq!(
        protocol_value_id(&snapshot.agent_config.options[0].current_value),
        Some("gpt-5.5")
    );
    assert_eq!(snapshot.agent_config.pending_change, None);
}

#[test]
fn set_config_option_is_a_noop_when_same_session_event_already_persisted_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("session-a".to_string());
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_set_config: AtomicBool::new(true),
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
    set_live_config_catalog(&store, "task-existing", config_catalog("gpt-5"));
    let setting_api = api.clone();
    let (result_tx, result_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        result_tx
            .send(
                setting_api.set_config_option_for_test(TaskSetConfigOptionParams {
                    task_id: "task-existing".into(),
                    config_id: "model".into(),
                    value: protocol_config_id("gpt-5.5"),
                    client_mutation_id: "same-session-change".into(),
                }),
            )
            .unwrap();
    });
    wait_until(|| agent.session_config_updates.lock().unwrap().len() == 1);
    let session_events = crate::tasks::turn_events::TaskSessionEventSink::new(
        api.mutations.clone(),
        "task-existing".to_string(),
        "session-a".to_string(),
        ServerRequestRuntime::new(),
    );
    session_events
        .config_options_changed(config_catalog("gpt-5.5"))
        .unwrap();
    let event_revision = store.read_task("task-existing").unwrap().revision;
    agent.block_set_config.store(false, Ordering::SeqCst);

    let snapshot = result_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("the reconciled session request should finish")
        .unwrap();
    let stored = store.read_task("task-existing").unwrap();

    assert_eq!(snapshot.revision, event_revision);
    assert_eq!(stored.revision, event_revision);
    assert_eq!(stored.model_id.as_deref(), Some("gpt-5.5"));
}

#[test]
fn set_config_option_preserves_agent_catalog_that_arrives_after_its_response_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("session-a".to_string());
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent {
        block_set_config: AtomicBool::new(true),
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
    set_live_config_catalog(&store, "task-existing", config_catalog("gpt-5"));
    let setting_api = api.clone();
    let (result_tx, result_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        result_tx
            .send(
                setting_api.set_config_option_for_test(TaskSetConfigOptionParams {
                    task_id: "task-existing".into(),
                    config_id: "model".into(),
                    value: protocol_config_id("gpt-5.5"),
                    client_mutation_id: "superseded-change".into(),
                }),
            )
            .unwrap();
    });
    wait_until(|| agent.session_config_updates.lock().unwrap().len() == 1);
    let session_events = crate::tasks::turn_events::TaskSessionEventSink::new(
        api.mutations.clone(),
        "task-existing".to_string(),
        "session-a".to_string(),
        ServerRequestRuntime::new(),
    );
    session_events
        .config_options_changed(config_catalog("gpt-5.4"))
        .unwrap();
    session_events
        .config_options_changed(config_catalog("gpt-5.2"))
        .unwrap();
    agent.block_set_config.store(false, Ordering::SeqCst);

    let snapshot = result_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("the superseded config request should reconcile")
        .unwrap();
    let stored = store.read_task("task-existing").unwrap();

    assert_eq!(
        protocol_value_id(&snapshot.agent_config.options[0].current_value),
        Some("gpt-5.2")
    );
    assert_eq!(snapshot.agent_config.pending_change, None);
    assert_eq!(stored.model_id.as_deref(), Some("gpt-5.2"));
}

#[test]
fn release_keeps_one_free_prepared_task_for_reuse() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", "/tmp/openaide-unit-workspace/app");
    draft.lifecycle = test_new_task_lifecycle();
    store.write_task(&draft).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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
    api.create_for_test(TaskAcquireParams {
        project_id: project_id_for_workspace("/tmp/openaide-unit-workspace/app"),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    })
    .unwrap();

    api.release_for_test(TaskReleaseParams {
        task_id: "task-draft".into(),
    })
    .unwrap();

    let released = store.read_task("task-draft").unwrap();
    assert!(!released.tombstoned);
    assert_eq!(released.lifecycle, TaskLifecycle::New { lease: None });
    assert!(!store.read_task("task-existing").unwrap().tombstoned);
}

#[test]
fn release_cleans_all_legacy_presend_attachment_resources_for_the_new_task() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let attachment_path = workspace.join("notes.txt");
    std::fs::write(&attachment_path, "hello").unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut draft = task_record("task-draft", workspace.to_string_lossy().as_ref());
    draft.lifecycle = test_new_task_lifecycle();
    store.write_task(&draft).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    api.create_for_test(TaskAcquireParams {
        project_id: project_id_for_workspace(workspace.to_string_lossy().as_ref()),
        agent_id: AgentId::from("codex"),
        workspace_root: None,
    })
    .unwrap();
    let attachments = api.attachment_runtime();
    let task_id = TaskId::from("task-draft");
    let handle =
        attachments.register_file_reference_for_test(task_id.clone(), "notes.txt", attachment_path);
    let root = attachments.list_roots(&task_id, &workspace).roots.remove(0);
    let listing = attachments
        .list_directory(&task_id, &workspace, &root.root_id, None)
        .unwrap();
    let candidate = attachments
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .candidate;

    api.release_for_test(TaskReleaseParams {
        task_id: task_id.clone(),
    })
    .unwrap();

    assert_eq!(
        attachments
            .refresh_handles(&task_id, &[handle.handle_id])
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
    let confirmation = attachments.confirm_embedded(&task_id, &[candidate.candidate_id]);
    assert_eq!(confirmation.errors.len(), 1);
}

#[test]
fn release_is_a_noop_for_visible_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-existing",
            "/tmp/openaide-unit-workspace/app",
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

    api.release_for_test(TaskReleaseParams {
        task_id: "task-existing".into(),
    })
    .unwrap();

    assert!(!store.read_task("task-existing").unwrap().tombstoned);
}

#[test]
fn release_is_a_noop_for_task_with_chat_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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

    api.release_for_test(TaskReleaseParams {
        task_id: "task-existing".into(),
    })
    .unwrap();

    assert!(!store.read_task("task-existing").unwrap().tombstoned);
}

#[test]
fn release_is_a_noop_for_running_visible_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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

    api.release_for_test(TaskReleaseParams {
        task_id: "task-existing".into(),
    })
    .unwrap();

    assert!(!store.read_task("task-existing").unwrap().tombstoned);
}

#[test]
fn release_is_idempotent_for_tombstoned_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
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

    api.release_for_test(TaskReleaseParams {
        task_id: "task-existing".into(),
    })
    .unwrap();

    assert!(store.read_task("task-existing").unwrap().tombstoned);
}

#[test]
fn send_rejects_tombstoned_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-existing", "/tmp/openaide-unit-workspace/app");
    record.lifecycle = test_new_task_lifecycle();
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

    let error = api.send(send_params("task-existing", "hello")).unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
    assert!(store.read_messages("task-existing").unwrap().is_empty());
}

#[test]
fn archiving_task_does_not_refresh_last_activity() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut older = task_record("task-old", "/tmp/openaide-unit-workspace/app");
    older.last_activity = "2026-01-01T00:00:00.000Z".to_string();
    older.updated_at = older.last_activity.clone();
    let mut newer_archived = task_record("task-newer-archived", "/tmp/openaide-unit-workspace/app");
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

    api.set_archived_for_test(TaskSetArchivedParams {
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
    // Product API tests use stable logical roots so Project ids remain readable.
    // Materialize them because production now rejects unavailable workspaces.
    std::fs::create_dir_all(workspace_root).unwrap();
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitle::new(
            "Existing",
            crate::storage::records::TaskTitleSource::User,
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace_root.to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: TaskLifecycle::Visible,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        archived: false,
        tombstoned: false,
        revision: 1,
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: true,
        preparation: TaskPreparationRecord::Ready,
    }
}

fn test_new_task_lifecycle() -> TaskLifecycle {
    TaskLifecycle::New {
        lease: Some(crate::attachment_runtime::AttachmentOwner::test_client_instance_id()),
    }
}

fn send_params(task_id: &str, text: &str) -> TaskSendParams {
    TaskSendParams {
        task_id: task_id.into(),
        message: ComposerMessage {
            text: Some(text.to_string()),
            images: Vec::new(),
            attachments: Vec::new(),
        },
    }
}

fn inline_image() -> ComposerImage {
    ComposerImage {
        label: "pasted.png".to_string(),
        mime_type: "image/png".to_string(),
        data: "aW1hZ2U=".to_string(),
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
    prompt_completions: AtomicUsize,
    steers: AtomicUsize,
    attaches: AtomicUsize,
    cancels: AtomicUsize,
    closes: AtomicUsize,
    list_calls: AtomicUsize,
    block_list: AtomicBool,
    fail_list: bool,
    block_start: AtomicBool,
    block_attach: AtomicBool,
    block_load: AtomicBool,
    block_close: AtomicBool,
    block_resume: AtomicBool,
    block_set_config: AtomicBool,
    config_requires_active_session: bool,
    resumed_session_active: AtomicBool,
    config_catalog: Option<ConfigOptionsCatalog>,
    commands_catalog: Option<AgentCommandsCatalog>,
    resume_config_catalog: Option<ConfigOptionsCatalog>,
    resume_commands_catalog: Option<AgentCommandsCatalog>,
    suppress_commands_on_attach: bool,
    listed_sessions: Mutex<Vec<AgentListedSession>>,
    replayed_messages: Mutex<Vec<NormalizedMessage>>,
    fail_start: bool,
    fail_attach: bool,
    fail_close: bool,
    fail_start_once: AtomicBool,
    fail_load_once_with_already_active: AtomicBool,
    resume_after_restart_unavailable: bool,
    active_after_load: AtomicBool,
    resume_session_missing: bool,
    load_start_timeout: bool,
    loaded_session_id: Option<String>,
    block_prompt: bool,
    hold_cancelled_prompt: AtomicBool,
    release_cancelled_prompt: AtomicBool,
    release_prompt: AtomicBool,
    prompt_calls: Mutex<Vec<(String, String)>>,
    steer_calls: Mutex<Vec<(String, String)>>,
    prompt_attachments: Mutex<Vec<Vec<Attachment>>>,
    session_config_updates: Mutex<Vec<(String, String, String)>>,
    typed_session_config_updates: Mutex<Vec<(String, String, ConfigOptionCurrentValue)>>,
}

#[derive(Default)]
struct OrderedConfigAgent {
    first_request_started: AtomicBool,
    release_first: AtomicBool,
    started_values: Mutex<Vec<String>>,
    completed_values: Mutex<Vec<String>>,
}

struct ImmediatePreparationCatalogAgent;

impl AgentRuntime for ImmediatePreparationCatalogAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "immediate-catalog-session",
        ))
    }

    fn attach_session_event_sink(
        &self,
        _session: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        sink.config_options_changed(config_catalog("gpt-5.5"))?;
        sink.commands_changed(command_catalog())
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
    }
}

impl AgentRuntime for OrderedConfigAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "ordered-session"))
    }

    fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        let value = request
            .value
            .as_id()
            .expect("ordered test uses select IDs")
            .to_string();
        self.started_values.lock().unwrap().push(value.clone());
        if value == "gpt-5.1" {
            self.first_request_started.store(true, Ordering::SeqCst);
            while !self.release_first.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
        }
        self.completed_values.lock().unwrap().push(value.clone());
        Ok(ordered_config_catalog(&value))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
    }
}

impl AgentRuntime for RecordingAgent {
    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        self.list_calls.fetch_add(1, Ordering::SeqCst);
        while self.block_list.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.fail_list {
            return Err(RuntimeError::NotReady("session listing failed".to_string()));
        }
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
        let session = AgentSession::new(request.agent_id, "recorded-session");
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
            return Err(RuntimeError::TaskNotFound(
                "Native Session missing-session".to_string(),
            ));
        }
        if self.resume_after_restart_unavailable && !self.active_after_load.load(Ordering::SeqCst) {
            return Err(RuntimeError::CapabilityMissing(
                "acp_session_resume_after_runtime_restart".to_string(),
            ));
        }
        self.resumed_session_active.store(true, Ordering::SeqCst);
        let session = AgentSession::new(request.agent_id, request.session_id);
        let session = match &self.resume_config_catalog {
            Some(catalog) => session.with_config_options(catalog),
            None => session,
        };
        Ok(match &self.resume_commands_catalog {
            Some(catalog) => session.with_commands_catalog(Some(catalog.clone())),
            None => session,
        })
    }

    fn load_session(&self, request: AgentSessionLoad) -> Result<AgentLoadedSession, RuntimeError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        while self.block_load.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
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
        let mut session = AgentSession::new(
            request.agent_id,
            self.loaded_session_id
                .clone()
                .unwrap_or_else(|| request.session_id.clone()),
        );
        if let Some(catalog) = &self.config_catalog {
            session = session.with_config_options(catalog);
        }
        session = session.with_commands_catalog(self.commands_catalog.clone());
        self.active_after_load.store(true, Ordering::SeqCst);
        Ok(AgentLoadedSession {
            session,
            replayed_messages: self.replayed_messages.lock().unwrap().clone(),
        })
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        self.prompt_attachments
            .lock()
            .unwrap()
            .push(prompt.attachments.clone());
        self.prompt_calls
            .lock()
            .unwrap()
            .push((prompt.session_id.clone(), prompt.text.clone()));
        while !prompt.cancellation.is_cancelled() {
            if !self.block_prompt || self.release_prompt.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let cancelled = prompt.cancellation.is_cancelled();
        while cancelled
            && self.hold_cancelled_prompt.load(Ordering::SeqCst)
            && !self.release_cancelled_prompt.load(Ordering::SeqCst)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let outcome = if cancelled {
            crate::agent::AgentPromptOutcome::Cancelled
        } else {
            crate::agent::AgentPromptOutcome::EndTurn
        };
        self.prompt_completions.fetch_add(1, Ordering::SeqCst);
        Ok(outcome)
    }

    fn steer(&self, prompt: AgentPrompt) -> Result<(), RuntimeError> {
        self.steers.fetch_add(1, Ordering::SeqCst);
        self.steer_calls
            .lock()
            .unwrap()
            .push((prompt.session_id, prompt.text));
        Ok(())
    }

    fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        if self.config_requires_active_session
            && !self.resumed_session_active.load(Ordering::SeqCst)
            && !self.active_after_load.load(Ordering::SeqCst)
        {
            return Err(RuntimeError::NotReady(
                "ACP session is not active".to_string(),
            ));
        }
        self.typed_session_config_updates.lock().unwrap().push((
            request.session_id.clone(),
            request.config_id.clone(),
            request.value.clone(),
        ));
        let response_catalog = match request.value {
            ConfigOptionCurrentValue::Id { value } => {
                self.session_config_updates.lock().unwrap().push((
                    request.session_id,
                    request.config_id,
                    value.clone(),
                ));
                config_catalog(&value)
            }
            ConfigOptionCurrentValue::Boolean { value } => boolean_config_catalog(value),
        };
        while self.block_set_config.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(response_catalog)
    }

    fn attach_session_event_sink(
        &self,
        _session: &AgentSessionKey,
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
        if !self.suppress_commands_on_attach {
            if let Some(catalog) = &self.commands_catalog {
                sink.commands_changed(catalog.clone())?;
            }
        }
        Ok(())
    }

    fn cancel_session(&self, _session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.cancels.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn close_session(&self, _session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        while self.block_close.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.fail_close {
            return Err(RuntimeError::NotReady("session close failed".to_string()));
        }
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
        Ok(AgentSession::new(request.agent_id, "secret-session"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
    }
}

#[derive(Default)]
struct PagedSessionAgent {
    requested_cursors: Mutex<Vec<Option<String>>>,
}

#[derive(Default)]
struct CyclingEmptySessionAgent {
    requested_cursors: Mutex<Vec<(String, Option<String>)>>,
}

impl CyclingEmptySessionAgent {
    fn requested_cursors(&self) -> Vec<Option<String>> {
        self.requested_cursors
            .lock()
            .unwrap()
            .iter()
            .map(|(_, cursor)| cursor.clone())
            .collect()
    }
}

impl AgentRuntime for CyclingEmptySessionAgent {
    fn list_sessions(
        &self,
        request: crate::agent::AgentListSessionsRequest,
    ) -> Result<crate::protocol::model::AgentListSessionsResult, RuntimeError> {
        let mut requested = self.requested_cursors.lock().unwrap();
        if requested
            .iter()
            .any(|(agent_id, cursor)| agent_id == &request.agent_id && cursor == &request.cursor)
        {
            return Err(RuntimeError::Internal(
                "repeated session cursor must not be requested".to_string(),
            ));
        }
        requested.push((request.agent_id.clone(), request.cursor.clone()));
        let next_cursor = match request.cursor.as_deref() {
            None => Some("page-2".to_string()),
            Some("page-2") => Some("page-3".to_string()),
            Some("page-3") => Some("page-2".to_string()),
            _ => None,
        };
        Ok(crate::protocol::model::AgentListSessionsResult {
            agent_id: request.agent_id,
            sessions: Vec::new(),
            next_cursor,
        })
    }

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "cycling-session"))
    }

    fn resume_session(&self, _request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(
            "acp_session_resume_after_runtime_restart".to_string(),
        ))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
    }
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

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "paged-session"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
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
            kind: ConfigOptionKind::Select,
            current_value: ConfigOptionCurrentValue::id(current_value),
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

fn ordered_config_catalog(current_value: &str) -> ConfigOptionsCatalog {
    let mut catalog = config_catalog(current_value);
    catalog.options[0].values.extend([
        ConfigOptionValue {
            id: "gpt-5.1".to_string(),
            label: "GPT 5.1".to_string(),
            description: None,
            group_id: None,
            group_label: None,
        },
        ConfigOptionValue {
            id: "gpt-5.2".to_string(),
            label: "GPT 5.2".to_string(),
            description: None,
            group_id: None,
            group_label: None,
        },
    ]);
    catalog
}

fn set_live_config_catalog(store: &Store, task_id: &str, catalog: ConfigOptionsCatalog) {
    let mut task = store.read_task(task_id).unwrap();
    task.config_options_catalog = Some(catalog);
    store.write_task(&task).unwrap();
}

fn boolean_config_catalog(current_value: bool) -> ConfigOptionsCatalog {
    ConfigOptionsCatalog {
        agent_id: "codex".to_string(),
        status: ConfigOptionsStatus::Ready,
        options: vec![ConfigOption {
            id: "brave_mode".to_string(),
            label: "Brave mode".to_string(),
            description: Some("Skip confirmation prompts".to_string()),
            category: Some(ConfigOptionCategory::Other),
            kind: ConfigOptionKind::Boolean,
            current_value: ConfigOptionCurrentValue::boolean(current_value),
            values: Vec::new(),
        }],
    }
}

fn mode_config_catalog(current_value: &str) -> ConfigOptionsCatalog {
    ConfigOptionsCatalog {
        agent_id: "codex".to_string(),
        status: ConfigOptionsStatus::Ready,
        options: vec![ConfigOption {
            id: "mode".to_string(),
            label: "Approval Preset".to_string(),
            description: None,
            category: Some(ConfigOptionCategory::Mode),
            kind: ConfigOptionKind::Select,
            current_value: ConfigOptionCurrentValue::id(current_value),
            values: vec![ConfigOptionValue {
                id: "agent-full-access".to_string(),
                label: "Full Access".to_string(),
                description: None,
                group_id: None,
                group_label: None,
            }],
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
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        let mut task = self.store.read_task("task-existing")?;
        task.config_options_catalog = Some(config_catalog("new-model"));
        task.model_id = Some("new-model".to_string());
        task.revision += 1;
        self.store.write_task(&task)?;
        Ok(AgentSession::new(request.agent_id, "mutating-session"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
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
