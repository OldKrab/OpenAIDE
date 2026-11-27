use openaide_app_server_protocol::ids::ProjectId;

use crate::native_sessions::catalog::{
    NativeSessionCatalog, NativeSessionObservation, NativeSessionRef,
};
use crate::navigation::{NavigationSource, SourceGroup};
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
fn background_work_shows_only_on_a_running_turn() {
    let held = |status| {
        let mut record = task_record("task-a", "Task A", "1000");
        record.status = status;
        record.active_turn_id = Some("turn-1".to_string());
        record.background_work = Some(crate::protocol::model::TaskBackgroundWork {
            held: true,
            commands: Vec::new(),
        });
        record
    };

    let summary = project_task_summary_with_has_messages(held(TaskStatus::Active), true);
    assert_eq!(summary.status, ProtocolTaskStatus::Background);

    // A permission wait or a Stop in the held turn is what the user must see.
    for (stored, visible) in [
        (TaskStatus::Waiting, ProtocolTaskStatus::Waiting),
        (TaskStatus::Stopping, ProtocolTaskStatus::Stopping),
    ] {
        let summary = project_task_summary_with_has_messages(held(stored), true);
        assert_eq!(summary.status, visible);
    }

    let mut settled = held(TaskStatus::Active);
    settled.active_turn_id = None;
    let summary = project_task_summary_with_has_messages(settled, true);
    assert_eq!(summary.status, ProtocolTaskStatus::Running);
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
fn projects_visible_task_records_into_navigation_groups() {
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

    let snapshot = TaskNavigationStore::new(store)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    let tasks = task_entries(&snapshot);
    assert_eq!(tasks.len(), 2);
    let newer = tasks
        .iter()
        .find(|task| task.task_id.as_str() == "task-newer")
        .expect("the newer Task is a member");
    assert_eq!(
        newer.title.as_ref().map(|title| title.value.as_str()),
        Some("New")
    );
    assert_eq!(newer.status, ProtocolTaskStatus::Idle);
    assert_eq!(newer.agent_id.as_str(), "agent-a");
    assert!(newer.project_id.as_str().starts_with("project-"));
}

#[test]
fn navigation_combines_durable_tasks_and_persisted_unadopted_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let task = task_record("task-owned", "Owned", "2026-07-20T00:00:00Z");
    let project_id = project_id_for_workspace(&task.workspace_root);
    store.write_task(&task).unwrap();
    let catalog = NativeSessionCatalog::open(store.clone()).unwrap();
    catalog
        .record_page(
            project_id.as_str(),
            &task.workspace_root,
            vec![NativeSessionObservation {
                reference: NativeSessionRef::new("agent-a", "native-new"),
                title: Some("Unadopted".to_string()),
                last_activity: Some("2026-07-21T00:00:00Z".to_string()),
            }],
        )
        .unwrap();

    let snapshot = TaskNavigationStore::with_native_sessions(store, catalog)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    // Row order belongs to the Navigation owner's one comparator, not to the source.
    assert_eq!(snapshot.groups[0].entries.len(), 2);
    assert!(snapshot.groups[0].entries.iter().any(|entry| matches!(
        entry,
        TaskNavigationEntry::NativeSession { session }
            if session.reference.session_id == "native-new" && session.title.as_deref() == Some("Unadopted")
    )));
    assert!(snapshot.groups[0].entries.iter().any(|entry| matches!(
        entry,
        TaskNavigationEntry::Task { task } if task.task_id.as_str() == "task-owned"
    )));
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
                timing: Default::default(),
            },
        )
        .unwrap();
    let mut record = store.read_task("task-1").unwrap();
    record.message_history_version = store.message_history_version("task-1").unwrap();
    store.write_task(&record).unwrap();

    let snapshot = TaskNavigationStore::new(store)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    assert_eq!(task_entries(&snapshot)[0].task_id.as_str(), "task-1");
    assert!(task_entries(&snapshot)[0].has_messages);
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

    let snapshot = TaskNavigationStore::new(store)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    assert_eq!(task_entries(&snapshot)[0].task_id.as_str(), "task-1");
    assert!(task_entries(&snapshot)[0].has_messages);
}

#[test]
fn a_task_nobody_has_written_to_is_not_a_member() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut empty = task_record("task-empty", "Empty", "2026-01-01T00:00:00.000Z");
    empty.message_history_version = 0;
    store.write_task(&empty).unwrap();
    store
        .write_task(&task_record(
            "task-written",
            "Written",
            "2026-01-02T00:00:00.000Z",
        ))
        .unwrap();

    let snapshot = TaskNavigationStore::new(store)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    let tasks = task_entries(&snapshot);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id.as_str(), "task-written");
}

#[test]
fn omits_archived_and_tombstoned_records() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut archived = task_record("task-archived", "Archived", "2026-01-02T00:00:00.000Z");
    archived.lifecycle = crate::storage::records::TaskLifecycle::Archived;
    let mut tombstoned = task_record("task-deleted", "Deleted", "2026-01-03T00:00:00.000Z");
    tombstoned.tombstoned = true;
    store.write_task(&archived).unwrap();
    store.write_task(&tombstoned).unwrap();

    let snapshot = TaskNavigationStore::new(store)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    assert!(task_entries(&snapshot).is_empty());
}

#[test]
fn archive_snapshot_contains_archived_tasks_and_archived_native_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let open = task_record("task-open", "Open", "2026-01-01T00:00:00.000Z");
    let mut archived = task_record("task-archived", "Archived", "2026-01-02T00:00:00.000Z");
    archived.lifecycle = crate::storage::records::TaskLifecycle::Archived;
    store.write_task(&open).unwrap();
    store.write_task(&archived).unwrap();
    let catalog = NativeSessionCatalog::open(store.clone()).unwrap();
    let native_reference = NativeSessionRef::new("agent-a", "native-archived");
    catalog
        .record_page(
            project_id_for_workspace(&open.workspace_root).as_str(),
            &open.workspace_root,
            vec![NativeSessionObservation {
                reference: native_reference.clone(),
                title: Some("Archived Native Session".to_string()),
                last_activity: Some("2026-01-03T00:00:00.000Z".to_string()),
            }],
        )
        .unwrap();
    catalog.archive(&native_reference).unwrap();

    let snapshot = TaskNavigationStore::with_native_sessions(store, catalog)
        .snapshot(TaskNavigationSection::Archive, None)
        .unwrap();

    assert_eq!(task_entries(&snapshot).len(), 1);
    assert_eq!(task_entries(&snapshot)[0].task_id.as_str(), "task-archived");
    assert!(snapshot
        .groups
        .iter()
        .flat_map(|group| &group.entries)
        .any(|entry| matches!(
            entry,
            TaskNavigationEntry::NativeSession { session }
                if session.reference.session_id == "native-archived"
        )));
}

#[test]
fn active_navigation_excludes_archived_native_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let catalog = NativeSessionCatalog::open(store.clone()).unwrap();
    let reference = NativeSessionRef::new("agent-a", "native-archived");
    catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: None,
                last_activity: None,
            }],
        )
        .unwrap();
    catalog.archive(&reference).unwrap();

    let snapshot = TaskNavigationStore::with_native_sessions(store, catalog)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    assert!(snapshot
        .groups
        .iter()
        .flat_map(|group| &group.entries)
        .all(|entry| !matches!(entry, TaskNavigationEntry::NativeSession { .. })));
}

#[test]
fn archived_task_keeps_its_native_session_out_of_primary_navigation() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut archived = task_record("task-archived", "Archived", "2026-01-02T00:00:00.000Z");
    archived.lifecycle = crate::storage::records::TaskLifecycle::Archived;
    archived.agent_session_id = Some("native-owned".to_string());
    let project_id = project_id_for_workspace(&archived.workspace_root);
    store.write_task(&archived).unwrap();
    let catalog = NativeSessionCatalog::open(store.clone()).unwrap();
    catalog
        .record_page(
            project_id.as_str(),
            &archived.workspace_root,
            vec![NativeSessionObservation {
                reference: NativeSessionRef::new("agent-a", "native-owned"),
                title: Some("Must stay owned".to_string()),
                last_activity: Some("2026-07-21T00:00:00Z".to_string()),
            }],
        )
        .unwrap();

    let snapshot = TaskNavigationStore::with_native_sessions(store, catalog)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    assert!(snapshot
        .groups
        .iter()
        .flat_map(|group| &group.entries)
        .all(|entry| !matches!(entry, TaskNavigationEntry::NativeSession { .. })));
}

#[test]
fn filters_by_project_id_when_requested() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let task = task_record("task-1", "Task", "2026-01-01T00:00:00.000Z");
    let included_project_id = project_id_for_workspace(&task.workspace_root);
    store.write_task(&task).unwrap();

    let included = TaskNavigationStore::new(store.clone())
        .snapshot(
            TaskNavigationSection::Tasks,
            Some(std::slice::from_ref(&included_project_id)),
        )
        .unwrap();
    let excluded = TaskNavigationStore::new(store)
        .snapshot(
            TaskNavigationSection::Tasks,
            Some(&[ProjectId::from("project-other")]),
        )
        .unwrap();

    assert_eq!(task_entries(&included).len(), 1);
    assert!(task_entries(&excluded).is_empty());
}

#[test]
fn storage_read_failure_is_isolated_from_navigation() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("corrupt", "Task", "1"))
        .unwrap();
    drop(store);
    corrupt_last_byte(&temp.path().join("task-store-v1/tasks/corrupt/task.json"));
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let snapshot = TaskNavigationStore::new(store)
        .snapshot(TaskNavigationSection::Tasks, None)
        .unwrap();

    assert!(task_entries(&snapshot).is_empty());
}

/// The source's groups, shaped like the old snapshot so the assertions read the same.
struct NavigationGroups {
    groups: Vec<SourceGroup>,
}

trait GroupsForTest {
    fn snapshot(
        &self,
        section: TaskNavigationSection,
        projects: Option<&[ProjectId]>,
    ) -> Result<NavigationGroups, ProtocolError>;
}

impl GroupsForTest for TaskNavigationStore {
    fn snapshot(
        &self,
        section: TaskNavigationSection,
        projects: Option<&[ProjectId]>,
    ) -> Result<NavigationGroups, ProtocolError> {
        let projects = projects.map(|projects| projects.iter().cloned().collect::<HashSet<_>>());
        Ok(NavigationGroups {
            groups: self.groups(section, projects.as_ref())?,
        })
    }
}

fn task_entries(snapshot: &NavigationGroups) -> Vec<&TaskSummary> {
    snapshot
        .groups
        .iter()
        .flat_map(|group| group.entries.iter())
        .filter_map(|entry| match entry {
            TaskNavigationEntry::Task { task } => Some(task.as_ref()),
            TaskNavigationEntry::NativeSession { .. } => None,
        })
        .collect()
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
        title: crate::storage::records::TaskTitleState::from_title(
            crate::storage::records::TaskTitle::new(
                title,
                crate::storage::records::TaskTitleSource::User,
            ),
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 1,
        unread: false,
        pinned: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: updated_at.to_string(),
        last_activity: updated_at.to_string(),
        permission_policy: Default::default(),
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
        background_work: None,
        tombstoned: false,
        revision: 1,
        config_options_catalog: None,
        native_session_data_freshness: Default::default(),
        native_session_reload_requirement: None,
        native_session_own_activity_at: None,
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
