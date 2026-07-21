use openaide_app_server_protocol::ids::ProjectId;

use crate::projects::project_id_for_workspace;
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{
    TaskAttentionEvent as StoredTaskAttentionEvent,
    TaskAttentionReason as StoredTaskAttentionReason, TaskPreparationRecord, TaskRecord,
};
use crate::storage::Store;

use super::*;

#[test]
fn task_summary_uses_canonical_project_identity() {
    let mut record = task_record("task-app", "Task", "2026-01-01T00:00:00.000Z");
    record.workspace_root = "/workspace/app/src/..".to_string();

    let summary = project_task_summary_with_has_messages(record, false);

    assert_eq!(
        summary.project_id,
        project_id_for_workspace("/workspace/app")
    );
}

#[test]
fn preparing_task_projects_preparing_status() {
    let mut record = task_record("task-preparing", "New task", "2026-01-01T00:00:00.000Z");
    record.preparation = TaskPreparationRecord::Preparing;

    let summary = project_task_summary_with_has_messages(record, false);

    assert_eq!(summary.status, ProtocolTaskStatus::Preparing);
}

#[test]
fn projects_durable_task_attention_into_navigation() {
    let mut record = task_record("task-attention", "Task", "2026-01-01T00:00:00.000Z");
    record.attention = Some(StoredTaskAttentionEvent::new(
        "attention-1",
        StoredTaskAttentionReason::NeedsPermission,
        "2026-01-01T00:01:00.000Z",
    ));

    let summary = project_task_summary_with_has_messages(record, true);

    assert!(matches!(
        summary.attention,
        Some(TaskAttentionEvent {
            event_id,
            reason: TaskAttentionReason::NeedsPermission,
            occurred_at,
        }) if event_id == "attention-1" && occurred_at == "2026-01-01T00:01:00.000Z"
    ));
}

#[test]
fn projects_visible_task_records_into_navigation_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-older",
            "Old",
            "2026-01-01T00:00:00.000Z",
        ))
        .unwrap();
    store
        .write_task(&task_record(
            "task-newer",
            "New",
            "2026-01-02T00:00:00.000Z",
        ))
        .unwrap();

    let snapshot = TaskNavigationStore::new(store).snapshot(None).unwrap();

    assert_eq!(snapshot.active_task_id, None);
    assert_eq!(snapshot.tasks.len(), 2);
    assert_eq!(snapshot.tasks[0].task_id.as_str(), "task-newer");
    assert_eq!(
        snapshot.tasks[0]
            .title
            .as_ref()
            .map(|title| title.value.as_str()),
        Some("New")
    );
    assert_eq!(snapshot.tasks[0].status, ProtocolTaskStatus::Idle);
    assert_eq!(snapshot.tasks[0].agent_id.as_str(), "agent-a");
    assert!(!snapshot.tasks[0].has_messages);
    assert!(snapshot.tasks[0]
        .project_id
        .as_str()
        .starts_with("project-"));
}

#[test]
fn marks_tasks_with_durable_chat_messages() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-1", "Task", "2026-01-01T00:00:00.000Z"))
        .unwrap();
    store
        .append_message(
            "task-1",
            crate::protocol::model::ChatMessage {
                cursor: "cursor-1".to_string(),
                identity: "user-1".to_string(),
                message_type: "user".to_string(),
                message_id: "user-1".to_string(),
                message: crate::protocol::model::NormalizedMessage::User {
                    id: "user-1".to_string(),
                    text: "hello".to_string(),
                    created_at: "2026-01-01T00:00:00.000Z".to_string(),
                    attachments: Vec::new(),
                },
            },
        )
        .unwrap();
    let mut record = store.read_task("task-1").unwrap();
    record.message_history_version = store.message_history_version("task-1").unwrap();
    store.write_task(&record).unwrap();

    let snapshot = TaskNavigationStore::new(store).snapshot(None).unwrap();

    assert_eq!(snapshot.tasks[0].task_id.as_str(), "task-1");
    assert!(snapshot.tasks[0].has_messages);
}

#[test]
fn navigation_uses_task_record_message_version_without_reading_chat_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-1", "Task", "2026-01-01T00:00:00.000Z");
    record.message_history_version = 7;
    store.write_task(&record).unwrap();
    assert!(!store
        .task_dir("task-1")
        .unwrap()
        .join("messages.jsonl")
        .exists());

    let snapshot = TaskNavigationStore::new(store).snapshot(None).unwrap();

    assert_eq!(snapshot.tasks[0].task_id.as_str(), "task-1");
    assert!(snapshot.tasks[0].has_messages);
}

#[test]
fn omits_archived_and_tombstoned_records() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut archived = task_record("task-archived", "Archived", "2026-01-02T00:00:00.000Z");
    archived.archived = true;
    let mut tombstoned = task_record("task-deleted", "Deleted", "2026-01-03T00:00:00.000Z");
    tombstoned.tombstoned = true;
    store.write_task(&archived).unwrap();
    store.write_task(&tombstoned).unwrap();

    let snapshot = TaskNavigationStore::new(store).snapshot(None).unwrap();

    assert!(snapshot.tasks.is_empty());
}

#[test]
fn filters_by_project_id_when_requested() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let task = task_record("task-1", "Task", "2026-01-01T00:00:00.000Z");
    let included_project_id = project_id_for_workspace(&task.workspace_root);
    store.write_task(&task).unwrap();

    let included = TaskNavigationStore::new(store.clone())
        .snapshot(Some(&included_project_id))
        .unwrap();
    let excluded = TaskNavigationStore::new(store)
        .snapshot(Some(&ProjectId::from("project-other")))
        .unwrap();

    assert_eq!(included.tasks.len(), 1);
    assert!(excluded.tasks.is_empty());
}

#[test]
fn storage_read_failure_is_isolated_from_navigation() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("corrupt", "Task", "1"))
        .unwrap();
    drop(store);
    corrupt_last_byte(&temp.path().join("task-store-v1/tasks/corrupt/task.journal"));
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let snapshot = TaskNavigationStore::new(store).snapshot(None).unwrap();

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

fn task_record(task_id: &str, title: &str, updated_at: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitle::new(
            title,
            crate::storage::records::TaskTitleSource::User,
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: updated_at.to_string(),
        last_activity: updated_at.to_string(),
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
        revision: 1,
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}
