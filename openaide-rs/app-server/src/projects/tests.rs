use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::Store;

use super::*;

#[test]
fn registered_project_survives_catalog_reopen() {
    let state = tempfile::tempdir().unwrap();
    let project_root = state.path().join("workspaces/app");
    std::fs::create_dir_all(&project_root).unwrap();
    let store = Store::open(state.path().join("state")).unwrap();

    let registered = ProjectCatalog::new(store.clone())
        .register_root(&project_root, None)
        .unwrap();
    let projects = ProjectCatalog::new(store)
        .projects(ProjectLifecycleFilter::Active)
        .unwrap();

    assert_eq!(projects, vec![registered.clone()]);
    assert_eq!(registered.label, "app");
    assert_eq!(registered.root, project_root.to_string_lossy());
    assert_eq!(registered.lifecycle, ProjectLifecycle::Active);
}

#[test]
fn registering_the_same_root_reuses_the_existing_project() {
    let state = tempfile::tempdir().unwrap();
    let project_root = state.path().join("workspaces/app");
    std::fs::create_dir_all(project_root.join("src")).unwrap();
    let catalog = ProjectCatalog::new(Store::open(state.path().join("state")).unwrap());

    let first = catalog.register_root(&project_root, None).unwrap();
    let repeated = catalog
        .register_root(&project_root.join("src/.."), Some("Replacement name"))
        .unwrap();

    assert_eq!(repeated, first);
    assert_eq!(
        catalog.projects(ProjectLifecycleFilter::All).unwrap(),
        vec![first]
    );
}

#[test]
fn removed_project_is_restored_when_its_root_is_registered_again() {
    let state = tempfile::tempdir().unwrap();
    let project_root = state.path().join("workspaces/app");
    std::fs::create_dir_all(&project_root).unwrap();
    let catalog = ProjectCatalog::new(Store::open(state.path().join("state")).unwrap());
    let registered = catalog.register_root(&project_root, None).unwrap();

    let removed = catalog.remove(&registered.project_id).unwrap();

    assert_eq!(removed.lifecycle, ProjectLifecycle::Removed);
    assert!(catalog
        .projects(ProjectLifecycleFilter::Active)
        .unwrap()
        .is_empty());
    assert_eq!(
        catalog.projects(ProjectLifecycleFilter::Removed).unwrap(),
        vec![removed]
    );

    let restored = catalog.register_root(&project_root, None).unwrap();

    assert_eq!(restored.project_id, registered.project_id);
    assert_eq!(restored.lifecycle, ProjectLifecycle::Active);
}

#[test]
fn first_catalog_read_migrates_project_from_archived_task_history() {
    let state = tempfile::tempdir().unwrap();
    let project_root = state.path().join("workspaces/legacy-app");
    std::fs::create_dir_all(&project_root).unwrap();
    let store = Store::open(state.path().join("state")).unwrap();
    let mut archived = task_record("task-archived", project_root.to_str().unwrap());
    archived.lifecycle = crate::storage::records::TaskLifecycle::Archived;
    store.write_task(&archived).unwrap();

    let projects = ProjectCatalog::new(store)
        .projects(ProjectLifecycleFilter::Active)
        .unwrap();

    assert_eq!(projects.len(), 1);
    assert_eq!(
        projects[0].project_id,
        project_id_for_workspace(project_root.to_str().unwrap())
    );
    assert_eq!(projects[0].label, "legacy-app");
    assert_eq!(projects[0].root, project_root.to_string_lossy());
}

#[test]
fn project_can_be_renamed_without_changing_identity_or_root() {
    let state = tempfile::tempdir().unwrap();
    let project_root = state.path().join("workspaces/app");
    std::fs::create_dir_all(&project_root).unwrap();
    let store = Store::open(state.path().join("state")).unwrap();
    let catalog = ProjectCatalog::new(store.clone());
    let registered = catalog.register_root(&project_root, None).unwrap();

    let renamed = catalog
        .rename(&registered.project_id, "  Client application  ")
        .unwrap();

    assert_eq!(renamed.project_id, registered.project_id);
    assert_eq!(renamed.root, registered.root);
    assert_eq!(renamed.label, "Client application");
    assert_eq!(
        ProjectCatalog::new(store)
            .projects(ProjectLifecycleFilter::Active)
            .unwrap(),
        vec![renamed]
    );
}

#[test]
fn project_can_reconnect_to_a_new_root_without_changing_identity() {
    let state = tempfile::tempdir().unwrap();
    let old_root = state.path().join("workspaces/old-app");
    let new_root = state.path().join("workspaces/moved-app");
    std::fs::create_dir_all(&old_root).unwrap();
    std::fs::create_dir_all(&new_root).unwrap();
    let catalog = ProjectCatalog::new(Store::open(state.path().join("state")).unwrap());
    let registered = catalog
        .register_root(&old_root, Some("Application"))
        .unwrap();

    let reconnected = catalog
        .reconnect(&registered.project_id, &new_root)
        .unwrap();

    assert_eq!(reconnected.project_id, registered.project_id);
    assert_eq!(reconnected.label, "Application");
    assert_eq!(reconnected.root, new_root.to_string_lossy());
    assert_eq!(reconnected.lifecycle, ProjectLifecycle::Active);
}

#[test]
fn task_context_resolution_uses_the_reconnected_project_root() {
    let state = tempfile::tempdir().unwrap();
    let old_root = state.path().join("workspaces/old-app");
    let new_root = state.path().join("workspaces/moved-app");
    std::fs::create_dir_all(&old_root).unwrap();
    std::fs::create_dir_all(&new_root).unwrap();
    let store = Store::open(state.path().join("state")).unwrap();
    let catalog = ProjectCatalog::new(store.clone());
    let registered = catalog
        .register_root(&old_root, Some("Application"))
        .unwrap();
    catalog
        .reconnect(&registered.project_id, &new_root)
        .unwrap();

    let context = StorageProjectResolver::new(store)
        .resolve_task_context(&registered.project_id)
        .unwrap();

    assert_eq!(context.project_id, registered.project_id);
    assert_eq!(context.workspace_root, new_root.to_string_lossy());
    assert_eq!(context.label, "Application");
}

#[test]
fn resolves_project_context_from_configured_roots_without_task_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let project_id = project_id_for_workspace("/workspace/app");

    let context = StorageProjectResolver::new_with_configured_roots(
        store,
        ConfiguredProjectRoots::from_workspace_roots(vec!["/workspace/app".to_string()]),
    )
    .resolve_task_context(&project_id)
    .unwrap();

    assert_eq!(context.project_id, project_id);
    assert_eq!(context.workspace_root, "/workspace/app");
    assert_eq!(context.label, "app");
    assert_eq!(context.isolation, IsolationKind::Local);
}

#[test]
fn resolves_project_context_from_existing_task_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("task-1", "/workspace/app"))
        .unwrap();
    let project_id = project_id_for_workspace("/workspace/app");

    let context = StorageProjectResolver::new(store)
        .resolve_task_context(&project_id)
        .unwrap();

    assert_eq!(context.project_id, project_id);
    assert_eq!(context.workspace_root, "/workspace/app");
    assert_eq!(context.label, "app");
    assert_eq!(context.isolation, IsolationKind::Local);
}

#[test]
fn resolves_project_context_from_canonical_workspace_identity() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut older = task_record("task-older", "/workspace/app/");
    older.last_activity = "2026-01-01T00:00:00.000Z".to_string();
    older.updated_at = older.last_activity.clone();
    let mut newer = task_record("task-newer", "/workspace/app/src/..");
    newer.last_activity = "2026-01-02T00:00:00.000Z".to_string();
    newer.updated_at = newer.last_activity.clone();
    store.write_task(&older).unwrap();
    store.write_task(&newer).unwrap();
    let project_id = project_id_for_workspace("/workspace/app");

    let context = StorageProjectResolver::new(store)
        .resolve_task_context(&project_id)
        .unwrap();

    assert_eq!(context.project_id, project_id);
    assert_eq!(context.workspace_root, "/workspace/app");
    assert_eq!(context.label, "app");
}

#[test]
fn unknown_project_returns_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let error = StorageProjectResolver::new(store)
        .resolve_task_context(&ProjectId::from("project-missing"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
    assert!(!error.recoverable);
}

#[test]
fn resolves_isolation_from_newest_matching_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut older = task_record("task-older", "/workspace/app");
    older.isolation = IsolationKind::Docker;
    older.last_activity = "2026-01-01T00:00:00.000Z".to_string();
    older.updated_at = older.last_activity.clone();
    let mut newer = task_record("task-newer", "/workspace/app");
    newer.isolation = IsolationKind::Local;
    newer.last_activity = "2026-01-02T00:00:00.000Z".to_string();
    newer.updated_at = newer.last_activity.clone();
    store.write_task(&newer).unwrap();
    store.write_task(&older).unwrap();
    let project_id = project_id_for_workspace("/workspace/app");

    let context = StorageProjectResolver::new(store)
        .resolve_task_context(&project_id)
        .unwrap();

    assert_eq!(context.isolation, IsolationKind::Local);
}

#[test]
fn corrupt_task_record_blocks_project_resolution() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("corrupt", "/workspace/app"))
        .unwrap();
    drop(store);
    corrupt_last_byte(&temp.path().join("task-store-v1/tasks/corrupt/task.json"));
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let error = StorageProjectResolver::new(store)
        .resolve_task_context(&ProjectId::from("project-any"))
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Internal);
    assert!(error.recoverable);
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
    file.sync_all().unwrap();
}

fn task_record(task_id: &str, workspace_root: &str) -> TaskRecord {
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
        agent_id: "agent-a".to_string(),
        agent_name: "Agent A".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace_root.to_string(),
        project_id: None,
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Open,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        tombstoned: false,
        revision: 1,
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
