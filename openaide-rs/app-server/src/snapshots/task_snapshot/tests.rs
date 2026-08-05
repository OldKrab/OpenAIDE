use openaide_app_server_protocol::snapshot::{
    ActivityStepSnapshot, MessagePart, TaskHistorySyncSnapshot, TaskPreparationSnapshot,
    TaskSendCapabilityState, TaskSetupBlockerKind, TaskStatus as ProtocolTaskStatus,
    ToolPermissionDecisionSnapshot,
};
use std::sync::{Arc, Mutex};

use crate::protocol::model::{
    ActivityStatus, ActivityStep, ActivityToolDetails, ActivityToolField, ActivityToolInput,
    ActivityToolLocation, ActivityToolValue, AgentMessagePart, AgentMessageRole, ChatMessage,
    IsolationKind, NormalizedMessage, TaskStatus, ToolPermissionDecision, ToolPermissionOutcome,
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
        .list(TaskListLifecycle::Open, None, None)
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
    new_task.lifecycle = crate::storage::records::TaskLifecycle::Prepared {
        lease: Some(openaide_app_server_protocol::ids::ClientInstanceId::from(
            "client-a",
        )),
    };
    store.write_task(&new_task).unwrap();

    let result = TaskSnapshotStore::new(store)
        .list(TaskListLifecycle::Open, None, None)
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
    task.lifecycle = crate::storage::records::TaskLifecycle::Open;
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
    task.lifecycle = crate::storage::records::TaskLifecycle::Open;
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
fn tool_image_preview_reads_supported_structured_paths_without_a_workspace_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let png_path = workspace.join("preview.png");
    let text_path = workspace.join("notes.png");
    let outside_path = outside.join("secret.png");
    let alternate_path = workspace.join("alternate.png");
    let jpeg_path = workspace.join("preview.jpeg");
    let gif_path = workspace.join("preview.gif");
    let webp_path = workspace.join("preview.webp");
    let oversized_path = workspace.join("oversized.png");
    std::fs::write(&png_path, b"\x89PNG\r\n\x1a\npreview").unwrap();
    std::fs::write(&text_path, b"not an image").unwrap();
    std::fs::write(&outside_path, b"\x89PNG\r\n\x1a\noutside").unwrap();
    std::fs::write(&alternate_path, b"\x89PNG\r\n\x1a\nalternate").unwrap();
    std::fs::write(&jpeg_path, b"\xff\xd8\xffpreview").unwrap();
    std::fs::write(&gif_path, b"GIF89apreview").unwrap();
    std::fs::write(&webp_path, b"RIFF\x04\x00\x00\x00WEBPpreview").unwrap();
    let mut oversized = vec![0; 5 * 1024 * 1024 + 1];
    oversized[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    std::fs::write(&oversized_path, oversized).unwrap();

    let store = Store::open(temp.path().join("state")).unwrap();
    let mut task = task_record("task-1");
    task.workspace_root = workspace.to_string_lossy().to_string();
    store.write_task(&task).unwrap();
    let valid_artifact = persist_tool_detail(
        &store,
        "task-1",
        "valid",
        png_path.to_string_lossy().as_ref(),
    );
    let non_image_artifact = persist_tool_detail(
        &store,
        "task-1",
        "non-image",
        text_path.to_string_lossy().as_ref(),
    );
    let outside_artifact = persist_tool_detail(
        &store,
        "task-1",
        "outside",
        outside_path.to_string_lossy().as_ref(),
    );
    #[cfg(unix)]
    let symlink_artifact = {
        let symlink_path = workspace.join("linked-secret.png");
        std::os::unix::fs::symlink(&outside_path, &symlink_path).unwrap();
        persist_tool_detail(
            &store,
            "task-1",
            "symlink-outside",
            symlink_path.to_string_lossy().as_ref(),
        )
    };
    let multiple_paths_artifact = persist_tool_detail_with_paths(
        &store,
        "task-1",
        "multiple-paths",
        &[
            text_path.to_string_lossy().as_ref(),
            alternate_path.to_string_lossy().as_ref(),
        ],
    );
    let supported_artifacts = [
        (
            persist_tool_detail(
                &store,
                "task-1",
                "jpeg",
                jpeg_path.to_string_lossy().as_ref(),
            ),
            "image/jpeg",
        ),
        (
            persist_tool_detail(&store, "task-1", "gif", gif_path.to_string_lossy().as_ref()),
            "image/gif",
        ),
        (
            persist_tool_detail(
                &store,
                "task-1",
                "webp",
                webp_path.to_string_lossy().as_ref(),
            ),
            "image/webp",
        ),
    ];
    let oversized_artifact = persist_tool_detail(
        &store,
        "task-1",
        "oversized",
        oversized_path.to_string_lossy().as_ref(),
    );
    let nested_path_artifact = persist_tool_detail_with_input(
        &store,
        "task-1",
        "nested-path",
        ActivityToolInput {
            command: Vec::new(),
            cwd: None,
            query: None,
            queries: Vec::new(),
            url: None,
            path: None,
            fields: vec![ActivityToolField {
                name: "arguments".to_string(),
                value: ActivityToolValue::Object {
                    fields: vec![ActivityToolField {
                        name: "path".to_string(),
                        value: ActivityToolValue::String {
                            value: jpeg_path.to_string_lossy().to_string(),
                        },
                    }],
                },
            }],
        },
    );
    let snapshots = TaskSnapshotStore::new(store);
    let client_id = openaide_app_server_protocol::ids::ClientInstanceId::from("test-client");

    let preview = snapshots
        .tool_image_preview_for_client(&client_id, &TaskId::from("task-1"), &valid_artifact)
        .expect("preview request")
        .expect("valid workspace image");

    assert_eq!(preview.label, "preview.png");
    assert_eq!(preview.media_type, "image/png");
    assert_eq!(
        preview.data_url,
        "data:image/png;base64,iVBORw0KGgpwcmV2aWV3"
    );
    assert!(snapshots
        .tool_image_preview_for_client(&client_id, &TaskId::from("task-1"), &non_image_artifact,)
        .expect("non-image request")
        .is_none());
    assert_eq!(
        snapshots
            .tool_image_preview_for_client(&client_id, &TaskId::from("task-1"), &outside_artifact,)
            .expect("outside request")
            .expect("outside image")
            .label,
        "secret.png"
    );
    #[cfg(unix)]
    assert_eq!(
        snapshots
            .tool_image_preview_for_client(&client_id, &TaskId::from("task-1"), &symlink_artifact,)
            .expect("outside symlink request")
            .expect("outside symlink image")
            .label,
        "linked-secret.png"
    );
    assert_eq!(
        snapshots
            .tool_image_preview_for_client(
                &client_id,
                &TaskId::from("task-1"),
                &multiple_paths_artifact,
            )
            .expect("multiple paths request")
            .expect("later valid image")
            .label,
        "alternate.png"
    );
    for (artifact_id, expected_media_type) in supported_artifacts {
        assert_eq!(
            snapshots
                .tool_image_preview_for_client(&client_id, &TaskId::from("task-1"), &artifact_id,)
                .expect("supported image request")
                .expect("supported image")
                .media_type,
            expected_media_type
        );
    }
    assert!(snapshots
        .tool_image_preview_for_client(&client_id, &TaskId::from("task-1"), &oversized_artifact,)
        .expect("oversized image request")
        .is_none());
    assert_eq!(
        snapshots
            .tool_image_preview_for_client(
                &client_id,
                &TaskId::from("task-1"),
                &nested_path_artifact,
            )
            .expect("nested path request")
            .expect("typed nested path image")
            .media_type,
        "image/jpeg"
    );

    std::fs::remove_file(png_path).unwrap();
    assert!(snapshots
        .tool_image_preview_for_client(&client_id, &TaskId::from("task-1"), &valid_artifact,)
        .expect("deleted image request")
        .is_none());
}

#[test]
fn client_snapshot_read_hides_another_clients_new_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-new");
    task.lifecycle = crate::storage::records::TaskLifecycle::Prepared {
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
        openaide_app_server_protocol::snapshot::TaskLifecycle::Prepared
    );
    assert_eq!(hidden.code, ProtocolErrorCode::NotFound);
}

fn persist_tool_detail(store: &Store, task_id: &str, activity_id: &str, path: &str) -> String {
    persist_tool_detail_with_paths(store, task_id, activity_id, &[path])
}

fn persist_tool_detail_with_paths(
    store: &Store,
    task_id: &str,
    activity_id: &str,
    paths: &[&str],
) -> String {
    let mut message = NormalizedMessage::Activity {
        id: activity_id.to_string(),
        title: "Read image".to_string(),
        status: ActivityStatus::Completed,
        created_at: "2026-01-01T00:00:01.000Z".to_string(),
        collapsed: true,
        steps: vec![ActivityStep::Tool {
            tool_call_id: Some(format!("tool-{activity_id}")),
            name: "read".to_string(),
            status: ActivityStatus::Completed,
            presentation: None,
            input_summary: None,
            output_preview: None,
            detail_artifact_id: None,
            details: Some(Box::new(ActivityToolDetails {
                locations: paths
                    .iter()
                    .map(|path| ActivityToolLocation {
                        path: (*path).to_string(),
                        line: None,
                    })
                    .collect(),
                content: Vec::new(),
                input: None,
                output: None,
            })),
            permission_outcomes: Vec::new(),
        }],
    };
    store
        .persist_tool_artifacts(task_id, &mut message)
        .expect("persist tool detail");
    let NormalizedMessage::Activity { steps, .. } = message else {
        unreachable!()
    };
    let ActivityStep::Tool {
        detail_artifact_id, ..
    } = &steps[0]
    else {
        unreachable!()
    };
    detail_artifact_id.clone().expect("artifact id")
}

fn persist_tool_detail_with_input(
    store: &Store,
    task_id: &str,
    activity_id: &str,
    input: ActivityToolInput,
) -> String {
    let mut message = NormalizedMessage::Activity {
        id: activity_id.to_string(),
        title: "Read image".to_string(),
        status: ActivityStatus::Completed,
        created_at: "2026-01-01T00:00:01.000Z".to_string(),
        collapsed: true,
        steps: vec![ActivityStep::Tool {
            tool_call_id: Some(format!("tool-{activity_id}")),
            name: "read".to_string(),
            status: ActivityStatus::Completed,
            presentation: None,
            input_summary: None,
            output_preview: None,
            detail_artifact_id: None,
            details: Some(Box::new(ActivityToolDetails {
                locations: Vec::new(),
                content: Vec::new(),
                input: Some(input),
                output: None,
            })),
            permission_outcomes: Vec::new(),
        }],
    };
    store
        .persist_tool_artifacts(task_id, &mut message)
        .expect("persist tool detail");
    let NormalizedMessage::Activity { steps, .. } = message else {
        unreachable!()
    };
    let ActivityStep::Tool {
        detail_artifact_id, ..
    } = &steps[0]
    else {
        unreachable!()
    };
    detail_artifact_id.clone().expect("artifact id")
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
        .list(TaskListLifecycle::Open, None, None)
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
            presentation: None,
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
        title: crate::storage::records::TaskTitleState::from_title(
            crate::storage::records::TaskTitle::new(
                "Task",
                crate::storage::records::TaskTitleSource::User,
            ),
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        pinned: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        composer_history: Default::default(),
        message_queue: Default::default(),
        agent_id: "agent-a".to_string(),
        agent_name: "Agent A".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace/a".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Open,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        tombstoned: false,
        revision: 7,
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        context_usage: None,
        current_plan: None,
        completed_plan_message_id: None,
        last_turn_usage: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}
