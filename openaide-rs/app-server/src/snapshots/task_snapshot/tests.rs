use openaide_app_server_protocol::snapshot::{
    MessagePart, PermissionMessageDecision, PermissionMessageState, TaskSendCapabilityState,
    TaskStatus as ProtocolTaskStatus,
};

use crate::protocol::model::{
    ActivityStatus, ActivityStep, ChatMessage, IsolationKind, NormalizedMessage,
    PermissionDecision, PermissionOption, PermissionOptionKind, PermissionState,
    PermissionToolCall, TaskStatus,
};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::Store;

use super::*;

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
fn open_projects_preparing_task_status() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-1");
    task.preparation = TaskPreparationRecord::Preparing;
    store.write_task(&task).unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open(&TaskId::from("task-1"))
        .expect("open");

    assert_eq!(snapshot.task.status, ProtocolTaskStatus::Preparing);
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
            chat_message(NormalizedMessage::AgentText {
                id: "agent-1".to_string(),
                text: "done".to_string(),
                created_at: "2026-01-01T00:00:01.000Z".to_string(),
                streaming: false,
            }),
        )
        .unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open(&TaskId::from("task-1"))
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
fn open_projects_durable_permission_history_as_permission_part() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-1")).unwrap();
    store
        .append_message(
            "task-1",
            chat_message(NormalizedMessage::Permission {
                id: "permission-1".to_string(),
                request_id: "agent-permission-1".to_string(),
                app_server_request_id: Some("server-request-1".to_string()),
                title: "Tool call".to_string(),
                description: None,
                scope: None,
                risk: None,
                tool_call: PermissionToolCall {
                    id: "call-1".to_string(),
                    title: "Tool call".to_string(),
                    kind: Some("execute".to_string()),
                },
                state: PermissionState::Resolved,
                created_at: "2026-01-01T00:00:01.000Z".to_string(),
                options: vec![
                    PermissionOption {
                        id: "allow_once".to_string(),
                        label: "Allow Once".to_string(),
                        kind: Some(PermissionOptionKind::Allow),
                        description: None,
                    },
                    PermissionOption {
                        id: "reject_once".to_string(),
                        label: "Reject".to_string(),
                        kind: Some(PermissionOptionKind::Deny),
                        description: None,
                    },
                ],
                selected_option: None,
                decision: Some(PermissionDecision::Denied),
            }),
        )
        .unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open(&TaskId::from("task-1"))
        .expect("open");

    let [MessagePart::Permission {
        request_id,
        app_server_request_id,
        title,
        tool_call,
        state,
        options,
        selected_option,
        decision,
        ..
    }] = snapshot.chat.items[0].parts.as_slice()
    else {
        panic!("expected permission message part");
    };
    assert_eq!(request_id.as_str(), "agent-permission-1");
    assert_eq!(
        app_server_request_id.as_ref().map(|id| id.as_str()),
        Some("server-request-1")
    );
    assert_eq!(title, "Tool call");
    assert_eq!(tool_call.kind.as_deref(), Some("execute"));
    assert_eq!(*state, PermissionMessageState::Resolved);
    assert_eq!(options[0].option_id, "allow_once");
    assert_eq!(selected_option, &None);
    assert_eq!(*decision, Some(PermissionMessageDecision::Denied));
}

#[test]
fn cancelled_pending_permission_projects_as_cancelled_not_denied() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store.write_task(&task_record("task-1")).unwrap();
    store
        .append_message(
            "task-1",
            chat_message(NormalizedMessage::Permission {
                id: "permission-1".to_string(),
                request_id: "agent-permission-1".to_string(),
                app_server_request_id: Some("server-request-1".to_string()),
                title: "Tool call".to_string(),
                description: None,
                scope: None,
                risk: None,
                tool_call: PermissionToolCall {
                    id: "call-1".to_string(),
                    title: "Tool call".to_string(),
                    kind: Some("execute".to_string()),
                },
                state: PermissionState::Pending,
                created_at: "2026-01-01T00:00:01.000Z".to_string(),
                options: vec![PermissionOption {
                    id: "allow_once".to_string(),
                    label: "Allow Once".to_string(),
                    kind: Some(PermissionOptionKind::Allow),
                    description: None,
                }],
                selected_option: None,
                decision: None,
            }),
        )
        .unwrap();
    store.cancel_pending_permissions("task-1").unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open(&TaskId::from("task-1"))
        .expect("open");

    let [MessagePart::Permission {
        state,
        selected_option,
        decision,
        ..
    }] = snapshot.chat.items[0].parts.as_slice()
    else {
        panic!("expected permission message part");
    };
    assert_eq!(*state, PermissionMessageState::Cancelled);
    assert_eq!(selected_option, &None);
    assert_eq!(*decision, None);
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
    task.first_prompt_sent = true;
    task.agent_session_id = Some("session-1".to_string());
    task.preparation = TaskPreparationRecord::Ready;
    store.write_task(&task).unwrap();

    let snapshot = TaskSnapshotStore::new(store)
        .open(&TaskId::from("task-1"))
        .expect("open");

    assert_eq!(snapshot.task.status, ProtocolTaskStatus::Failed);
    assert_eq!(
        snapshot.send_capability.state,
        TaskSendCapabilityState::Ready
    );
    assert!(snapshot.send_capability.attachment_only);
    assert!(snapshot.send_capability.blockers.is_empty());
}

#[test]
fn missing_task_returns_not_found_error() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let error = TaskSnapshotStore::new(store)
        .open(&TaskId::from("missing"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
    assert!(!error.recoverable);
}

#[test]
fn list_returns_error_for_corrupt_task_record() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(store.tasks_dir().join("corrupt")).unwrap();
    std::fs::write(store.tasks_dir().join("corrupt/task.json"), "{not-json").unwrap();

    let error = TaskSnapshotStore::new(store)
        .list(false, None, None)
        .expect_err("corrupt task record should fail list");

    assert_eq!(error.code, ProtocolErrorCode::Internal);
    assert!(error.recoverable);
    assert!(error.message.contains("Failed to read task navigation"));
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
        }],
    }
}

fn activity_status(message: &NormalizedMessage) -> Option<ActivityStatus> {
    match message {
        NormalizedMessage::Activity { status, .. } => Some(*status),
        _ => None,
    }
}

fn task_record(task_id: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: "Task".to_string(),
        agent_title: None,
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "agent-a".to_string(),
        agent_name: "Agent A".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace/a".to_string(),
        first_prompt_sent: false,
        agent_session_id: None,
        active_turn_id: None,
        archived: false,
        tombstoned: false,
        revision: 7,
        config_options: Default::default(),
        config_options_catalog: None,
        agent_commands_catalog: None,
        model_id: None,
        preparation: TaskPreparationRecord::Ready,
    }
}
