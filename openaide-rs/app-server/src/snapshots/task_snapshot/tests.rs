use openaide_app_server_protocol::snapshot::{
    ActivityStepSnapshot, MessagePart, TaskHistorySyncSnapshot, TaskPreparationSnapshot,
    TaskSendCapabilityState, TaskSetupBlockerKind, TaskStatus as ProtocolTaskStatus,
    ToolPermissionDecisionSnapshot,
};
use std::sync::{Arc, Mutex};

use crate::protocol::model::{
    ActivityStatus, ActivityStep, AgentMessagePart, AgentMessageRole, ChatMessage, IsolationKind,
    NormalizedMessage, TaskStatus, ToolPermissionDecision, ToolPermissionOutcome,
};
use crate::storage::records::{TaskPreparationBlockerRecord, TaskPreparationRecord, TaskRecord};
use crate::storage::Store;

use super::*;

#[derive(Clone)]
struct MutableHistorySyncSource {
    current: Arc<Mutex<TaskHistorySyncSnapshot>>,
}

impl MutableHistorySyncSource {
    fn new(current: TaskHistorySyncSnapshot) -> Self {
        Self {
            current: Arc::new(Mutex::new(current)),
        }
    }

    fn set(&self, current: TaskHistorySyncSnapshot) {
        *self.current.lock().unwrap() = current;
    }
}

impl TaskHistorySyncSnapshotSource for MutableHistorySyncSource {
    fn history_sync_snapshot(&self, _task_id: &str) -> TaskHistorySyncSnapshot {
        self.current.lock().unwrap().clone()
    }
}

#[test]
fn list_projects_visible_tasks_and_revision() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-1")).unwrap();

    let result = TaskSnapshotStore::new(store)
        .list(false, None, None)
        .expect("list");

    assert_eq!(result.tasks.len(), 1);
    assert_eq!(result.tasks[0].task_id.as_str(), "task-1");
    assert_eq!(result.revision, 7);
    assert_eq!(result.next_cursor, None);
}

#[test]
fn list_revision_ignores_client_private_new_tasks() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-visible")).unwrap();
    let mut new_task = task_record("task-new");
    new_task.revision = 99;
    new_task.lifecycle = crate::storage::records::TaskLifecycle::New {
        lease: Some(openaide_app_server_protocol::ids::ClientInstanceId::from(
            "client-a",
        )),
    };
    store.write_task(&new_task).unwrap();

    let result = TaskSnapshotStore::new(store)
        .list(false, None, None)
        .expect("list");

    assert_eq!(result.revision, 7);
}

#[test]
fn open_projects_preparing_task_status() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-1");
    task.preparation = TaskPreparationRecord::Preparing;
    store.write_task(&task).unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open_internal(&TaskId::from("task-1"))
        .expect("open");

    assert_eq!(snapshot.task.status, ProtocolTaskStatus::Preparing);
}

#[test]
fn open_projects_node_js_setup_as_a_recoverable_blocker() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-1");
    task.preparation = TaskPreparationRecord::Blocked {
        reason: TaskPreparationBlockerRecord::NodeJsRequired,
        message: "Node.js tools are unavailable".to_string(),
    };
    store.write_task(&task).unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open_internal(&TaskId::from("task-1"))
        .expect("open");

    let TaskPreparationSnapshot::Blocked { blocker, actions } = snapshot.preparation else {
        panic!("expected blocked preparation");
    };
    assert_eq!(blocker.kind, TaskSetupBlockerKind::NodeJsRequired);
    assert_eq!(blocker.message, "Node.js tools are unavailable");
    assert!(actions.contains(&openaide_app_server_protocol::snapshot::TaskPreparationAction::Retry));
}

#[test]
fn open_overlays_current_history_sync_state_for_resubscribe() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-1")).unwrap();
    let history_sync =
        MutableHistorySyncSource::new(TaskHistorySyncSnapshot::Syncing { generation: 7 });
    let snapshots =
        TaskSnapshotStore::with_history_sync(store.clone(), Arc::new(history_sync.clone()));

    let syncing = snapshots
        .open_internal(&TaskId::from("task-1"))
        .expect("open syncing");

    assert_eq!(
        syncing.history_sync,
        TaskHistorySyncSnapshot::Syncing { generation: 7 }
    );

    let mut task = store.read_task("task-1").unwrap();
    task.unread = !task.unread;
    task.revision += 1;
    store.write_task(&task).unwrap();
    history_sync.set(TaskHistorySyncSnapshot::Idle { generation: 7 });

    let idle = snapshots
        .open_internal(&TaskId::from("task-1"))
        .expect("open after unrelated mutation");

    assert_eq!(
        idle.history_sync,
        TaskHistorySyncSnapshot::Idle { generation: 7 }
    );

    history_sync.set(TaskHistorySyncSnapshot::Updated { generation: 7 });
    let updated = snapshots
        .open_internal(&TaskId::from("task-1"))
        .expect("resubscribe after history update");
    assert_eq!(
        updated.history_sync,
        TaskHistorySyncSnapshot::Updated { generation: 7 }
    );
}

#[test]
fn open_projects_durable_chat_without_raw_attachment_paths() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-1")).unwrap();
    store
        .append_message(
            "task-1",
            chat_message(NormalizedMessage::User {
                id: "user-1".to_string(),
                text: "hello".to_string(),
                created_at: "2026-01-01T00:00:00.000Z".to_string(),
                attachments: vec![crate::protocol::model::Attachment {
                    kind: "file".to_string(),
                    label: "main.rs".to_string(),
                    path: Some("/secret/workspace/main.rs".to_string()),
                    payload: None,
                }],
            }),
        )
        .unwrap();
    store
        .append_message(
            "task-1",
            chat_message(NormalizedMessage::AgentMessage {
                id: "agent-1".to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: "done".to_string(),
                }],
                created_at: "2026-01-01T00:00:01.000Z".to_string(),
            }),
        )
        .unwrap();
    sync_task_message_history_version(&store, "task-1");

    let snapshot = TaskSnapshotStore::new(store)
        .open_internal(&TaskId::from("task-1"))
        .expect("open");

    assert_eq!(snapshot.task.task_id.as_str(), "task-1");
    assert_eq!(snapshot.revision, 7);
    assert_eq!(snapshot.chat.items.len(), 2);
    assert_eq!(
        snapshot
            .chat
            .start_cursor
            .as_ref()
            .map(|cursor| cursor.as_str()),
        Some("cursor-1")
    );
    assert_eq!(
        snapshot
            .chat
            .end_cursor
            .as_ref()
            .map(|cursor| cursor.as_str()),
        Some("cursor-1")
    );
    assert!(snapshot.task.has_messages);
    assert!(snapshot.chat.has_messages);
    let rendered = serde_json::to_string(&snapshot).unwrap();
    assert!(rendered.contains("main.rs"));
    assert!(!rendered.contains("/secret/workspace"));
}

#[test]
fn open_retries_when_message_commit_interleaves_with_snapshot_read() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-1")).unwrap();
    let interleaving_store = store.clone();
    store.after_next_task_snapshot_read_for_test(move || {
        interleaving_store
            .append_message(
                "task-1",
                chat_message(NormalizedMessage::AgentMessage {
                    id: "agent-1".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: "committed while reading".to_string(),
                    }],
                    created_at: "2026-01-01T00:00:01.000Z".to_string(),
                }),
            )
            .unwrap();
        let mut committed_task = interleaving_store.read_task("task-1").unwrap();
        committed_task.message_history_version = interleaving_store
            .message_history_version("task-1")
            .unwrap();
        committed_task.revision = 8;
        interleaving_store.write_task(&committed_task).unwrap();
    });

    let snapshot = TaskSnapshotStore::new(store)
        .open_internal(&TaskId::from("task-1"))
        .expect("consistent snapshot");

    assert_eq!(snapshot.revision, 8);
    assert_eq!(snapshot.chat.items.len(), 1);
}

#[test]
fn open_projects_tool_permission_history_inside_activity_part() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-1")).unwrap();
    let mut activity = activity_message("activity-1", "call-1");
    let NormalizedMessage::Activity { steps, .. } = &mut activity else {
        unreachable!("activity fixture must be an activity message");
    };
    let ActivityStep::Tool {
        permission_outcomes,
        ..
    } = &mut steps[0]
    else {
        unreachable!("activity fixture must contain a tool step");
    };
    permission_outcomes.push(ToolPermissionOutcome {
        request_id: "server-request-1".to_string(),
        decision: ToolPermissionDecision::Rejected,
        option_id: Some("reject_once".to_string()),
        option_label: Some("Reject".to_string()),
        resolved_at: "2026-01-01T00:00:02.000Z".to_string(),
    });
    store
        .append_message("task-1", chat_message(activity))
        .unwrap();
    sync_task_message_history_version(&store, "task-1");

    let snapshot = TaskSnapshotStore::new(store)
        .open_internal(&TaskId::from("task-1"))
        .expect("open");

    let [MessagePart::Activity { steps, .. }] = snapshot.chat.items[0].parts.as_slice() else {
        panic!("expected activity message part");
    };
    let [ActivityStepSnapshot::Tool {
        permission_outcomes,
        ..
    }] = steps.as_slice()
    else {
        panic!("expected tool activity step");
    };
    assert_eq!(permission_outcomes.len(), 1);
    assert_eq!(
        permission_outcomes[0].request_id.as_str(),
        "server-request-1"
    );
    assert_eq!(
        permission_outcomes[0].decision,
        ToolPermissionDecisionSnapshot::Rejected
    );
    assert_eq!(
        permission_outcomes[0].option_id.as_deref(),
        Some("reject_once")
    );
}

#[test]
fn finish_running_activities_updates_all_concurrent_running_rows() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-1")).unwrap();
    store
        .append_message(
            "task-1",
            chat_message(activity_message("activity-1", "tool-1")),
        )
        .unwrap();
    store
        .append_message(
            "task-1",
            chat_message(activity_message("activity-2", "tool-2")),
        )
        .unwrap();

    store
        .finish_running_activities("task-1", ActivityStatus::Completed)
        .unwrap();

    let messages = store.read_messages("task-1").unwrap();
    assert_eq!(
        activity_status(&messages[0].chat.message),
        Some(ActivityStatus::Completed)
    );
    assert_eq!(
        activity_status(&messages[1].chat.message),
        Some(ActivityStatus::Completed)
    );
}

#[test]
fn failed_task_with_ready_preparation_is_sendable_for_follow_up_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-1");
    task.status = TaskStatus::Failed;
    task.lifecycle = crate::storage::records::TaskLifecycle::Visible;
    task.agent_session_id = Some("session-1".to_string());
    task.preparation = TaskPreparationRecord::Ready;
    store.write_task(&task).unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open_internal(&TaskId::from("task-1"))
        .expect("open");

    assert_eq!(snapshot.task.status, ProtocolTaskStatus::Failed);
    assert_eq!(
        snapshot.send_capability.state,
        TaskSendCapabilityState::Ready
    );
    assert!(snapshot.send_capability.blockers.is_empty());
}

#[test]
fn working_task_is_sendable_for_steering() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-1");
    task.status = TaskStatus::Active;
    task.lifecycle = crate::storage::records::TaskLifecycle::Visible;
    task.agent_session_id = Some("session-1".to_string());
    task.active_turn_id = Some("turn-primary".to_string());
    task.active_turn_started_at = Some("2026-07-13T00:00:00Z".to_string());
    task.preparation = TaskPreparationRecord::Ready;
    store.write_task(&task).unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open_internal(&TaskId::from("task-1"))
        .expect("open");

    assert_eq!(snapshot.task.status, ProtocolTaskStatus::Running);
    assert_eq!(
        snapshot.active_turn_started_at.as_deref(),
        Some("2026-07-13T00:00:00Z")
    );
    assert_eq!(
        snapshot.send_capability.state,
        TaskSendCapabilityState::Ready
    );
    assert!(snapshot.send_capability.blockers.is_empty());
}

#[test]
fn missing_task_returns_not_found_error() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let error = TaskSnapshotStore::new(store)
        .open_internal(&TaskId::from("missing"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
    assert!(!error.recoverable);
}

#[test]
fn client_snapshot_read_hides_another_clients_new_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-new");
    task.lifecycle = crate::storage::records::TaskLifecycle::New {
        lease: Some(openaide_app_server_protocol::ids::ClientInstanceId::from(
            "test-client",
        )),
    };
    store.write_task(&task).unwrap();
    let snapshots = TaskSnapshotStore::new(store);

    let owner = snapshots
        .open_for_client(
            &openaide_app_server_protocol::ids::ClientInstanceId::from("test-client"),
            &TaskId::from("task-new"),
        )
        .unwrap();
    let hidden = snapshots
        .open_for_client(
            &openaide_app_server_protocol::ids::ClientInstanceId::from("other-client"),
            &TaskId::from("task-new"),
        )
        .unwrap_err();

    assert_eq!(
        owner.lifecycle,
        openaide_app_server_protocol::snapshot::TaskLifecycle::New
    );
    assert_eq!(hidden.code, ProtocolErrorCode::NotFound);
}

#[test]
fn list_omits_a_corrupt_task_record() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("corrupt")).unwrap();
    drop(store);
    corrupt_last_byte(&temp.path().join("task-store-v1/tasks/corrupt/task.json"));
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .list(false, None, None)
        .expect("corrupt Task must stay isolated from collection reads");

    assert!(snapshot.tasks.is_empty());
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

fn chat_message(message: NormalizedMessage) -> ChatMessage {
    ChatMessage {
        cursor: "cursor-1".to_string(),
        identity: message.identity(),
        message_type: message.message_type().to_string(),
        message_id: message.identity(),
        message,
    }
}

fn activity_message(id: &str, tool_call_id: &str) -> NormalizedMessage {
    NormalizedMessage::Activity {
        id: id.to_string(),
        title: id.to_string(),
        status: ActivityStatus::Running,
        created_at: "2026-01-01T00:00:01.000Z".to_string(),
        collapsed: true,
        steps: vec![ActivityStep::Tool {
            tool_call_id: Some(tool_call_id.to_string()),
            name: "shell".to_string(),
            status: ActivityStatus::Running,
            input_summary: None,
            output_preview: None,
            detail_artifact_id: None,
            details: None,
            permission_outcomes: Vec::new(),
        }],
    }
}

fn activity_status(message: &NormalizedMessage) -> Option<ActivityStatus> {
    match message {
        NormalizedMessage::Activity { status, .. } => Some(*status),
        _ => None,
    }
}

fn sync_task_message_history_version(store: &Store, task_id: &str) {
    let mut task = store.read_task(task_id).unwrap();
    task.message_history_version = store.message_history_version(task_id).unwrap();
    store.write_task(&task).unwrap();
}

fn task_record(task_id: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitle::new(
            "Task",
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
        agent_id: "agent-a".to_string(),
        agent_name: "Agent A".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace/a".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Visible,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        archived: false,
        tombstoned: false,
        revision: 7,
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}
