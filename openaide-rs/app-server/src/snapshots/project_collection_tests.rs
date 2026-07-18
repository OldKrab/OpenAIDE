use openaide_app_server_protocol::errors::ProtocolErrorCode;
use std::process::Command;

use crate::projects::project_id_for_workspace;
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::Store;

use super::*;

#[test]
fn includes_configured_roots_without_task_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let snapshot = ProjectCollectionStore::new_with_configured_roots(
        store,
        ConfiguredProjectRoots::from_workspace_roots(vec!["/workspace/app".to_string()]),
    )
    .snapshot()
    .unwrap();

    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].label, "app");
    assert_eq!(
        snapshot.projects[0].project_id,
        project_id_for_workspace("/workspace/app")
    );
    assert!(!snapshot.projects[0].available);
    assert_eq!(snapshot.projects[0].workspace_root, "/workspace/app");
    assert!(snapshot.projects[0].worktree_repository_id.is_none());
}

#[test]
fn configured_git_root_projects_worktree_repository_identity() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-b", "main"]);
    git(&project, &["config", "user.name", "OpenAIDE Test"]);
    git(&project, &["config", "user.email", "test@openaide.invalid"]);
    std::fs::write(project.join("README.md"), "fixture\n").unwrap();
    git(&project, &["add", "README.md"]);
    git(&project, &["commit", "-m", "fixture"]);
    let state = tempfile::tempdir().unwrap();
    let store = Store::open(state.path().to_path_buf()).unwrap();

    let snapshot = ProjectCollectionStore::new_with_configured_roots(
        store,
        ConfiguredProjectRoots::from_workspace_roots(vec![project.to_string_lossy().to_string()]),
    )
    .snapshot()
    .unwrap();

    assert!(snapshot.projects[0].available);
    assert!(snapshot.projects[0].worktree_repository_id.is_some());
    assert!(snapshot.projects[0].project_worktree_id.is_some());
}

#[test]
fn projects_visible_task_records_into_collection_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-app",
            "/workspace/app",
            "2026-01-02T00:00:00.000Z",
        ))
        .unwrap();
    store
        .write_task(&task_record(
            "task-lib",
            "/workspace/lib",
            "2026-01-01T00:00:00.000Z",
        ))
        .unwrap();

    let snapshot = ProjectCollectionStore::new(store).snapshot().unwrap();

    assert_eq!(snapshot.projects.len(), 2);
    assert_eq!(snapshot.projects[0].label, "app");
    assert_eq!(
        snapshot.projects[0].project_id,
        project_id_for_workspace("/workspace/app")
    );
}

#[test]
fn deduplicates_projects_by_workspace_root() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-older",
            "/workspace/app",
            "2026-01-01T00:00:00.000Z",
        ))
        .unwrap();
    store
        .write_task(&task_record(
            "task-newer",
            "/workspace/app",
            "2026-01-02T00:00:00.000Z",
        ))
        .unwrap();

    let snapshot = ProjectCollectionStore::new(store).snapshot().unwrap();

    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].label, "app");
}

#[test]
fn deduplicates_projects_by_canonical_workspace_root() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record(
            "task-older",
            "/workspace/app/",
            "2026-01-01T00:00:00.000Z",
        ))
        .unwrap();
    store
        .write_task(&task_record(
            "task-newer",
            "/workspace/app/src/..",
            "2026-01-02T00:00:00.000Z",
        ))
        .unwrap();

    let snapshot = ProjectCollectionStore::new(store).snapshot().unwrap();

    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].label, "app");
    assert_eq!(
        snapshot.projects[0].project_id,
        project_id_for_workspace("/workspace/app")
    );
}

#[test]
fn omits_archived_and_tombstoned_records() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut archived = task_record(
        "task-archived",
        "/workspace/archived",
        "2026-01-01T00:00:00.000Z",
    );
    archived.archived = true;
    let mut tombstoned = task_record(
        "task-deleted",
        "/workspace/deleted",
        "2026-01-02T00:00:00.000Z",
    );
    tombstoned.tombstoned = true;
    store.write_task(&archived).unwrap();
    store.write_task(&tombstoned).unwrap();

    let snapshot = ProjectCollectionStore::new(store).snapshot().unwrap();

    assert!(snapshot.projects.is_empty());
}

#[test]
fn storage_read_failure_returns_recoverable_error() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    std::fs::remove_dir_all(store.tasks_dir()).unwrap();

    let error = ProjectCollectionStore::new(store).snapshot().unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::Internal);
    assert!(error.recoverable);
    assert!(error.message.contains("Failed to read project collection"));
}

fn task_record(task_id: &str, workspace_root: &str, updated_at: &str) -> TaskRecord {
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
        updated_at: updated_at.to_string(),
        last_activity: updated_at.to_string(),
        agent_id: "agent-a".to_string(),
        agent_name: "Agent A".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace_root.to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Visible,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        archived: false,
        tombstoned: false,
        revision: 1,
        config_options: Default::default(),
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}

fn git(cwd: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
