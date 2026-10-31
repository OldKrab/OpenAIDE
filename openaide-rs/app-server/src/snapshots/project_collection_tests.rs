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

fn committed_repository(root: &std::path::Path) {
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.name", "OpenAIDE Test"]);
    git(root, &["config", "user.email", "test@openaide.invalid"]);
    std::fs::write(root.join("README.md"), "fixture\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-m", "fixture"]);
}

fn collection_for(store: &Store, root: &std::path::Path) -> ProjectCollectionStore {
    ProjectCollectionStore::new_with_configured_roots(
        store.clone(),
        ConfiguredProjectRoots::from_workspace_roots(vec![root.to_string_lossy().to_string()]),
    )
}

/// Log capture spans parallel tests, so only this repository's refreshes count.
fn discovery_runs(logs: &crate::logging::TestLogCapture, summary: &ProjectSummary) -> usize {
    let repository_id = summary.worktree_repository_id.as_ref().unwrap().as_str();
    logs.snapshot()
        .iter()
        .filter(|entry| entry["event"] == "worktree_repository_refreshed")
        .filter(|entry| entry["fields"]["repository_id"] == repository_id)
        .count()
}

#[test]
fn a_cataloged_project_answers_without_running_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    committed_repository(&project);
    let state = tempfile::tempdir().unwrap();
    let store = Store::open(state.path().to_path_buf()).unwrap();
    let collection = collection_for(&store, &project);
    let first = collection.snapshot().unwrap();
    let logs = crate::logging::capture_test_logs();

    let second = collection.snapshot().unwrap();

    assert_eq!(discovery_runs(&logs, &first.projects[0]), 0);
    assert!(second.projects[0].available);
    assert_eq!(
        second.projects[0].worktree_repository_id,
        first.projects[0].worktree_repository_id
    );
    assert_eq!(
        second.projects[0].project_worktree_id,
        first.projects[0].project_worktree_id
    );
}

#[test]
fn a_project_that_stopped_being_a_checkout_loses_its_repository() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    committed_repository(&project);
    let state = tempfile::tempdir().unwrap();
    let store = Store::open(state.path().to_path_buf()).unwrap();
    let collection = collection_for(&store, &project);
    collection.snapshot().unwrap();

    std::fs::remove_dir_all(project.join(".git")).unwrap();
    let snapshot = collection.snapshot().unwrap();

    assert!(snapshot.projects[0].available);
    assert!(snapshot.projects[0].worktree_repository_id.is_none());
}

#[test]
fn a_recreated_worktree_runs_discovery_again() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let linked = temp.path().join("linked");
    committed_repository(&project);
    git(
        &project,
        &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
    );
    let state = tempfile::tempdir().unwrap();
    let store = Store::open(state.path().to_path_buf()).unwrap();
    // A linked Task keeps the record through the refresh, which would discard it otherwise.
    store
        .write_task(&task_record(
            "task-linked",
            linked.to_str().unwrap(),
            "2026-01-01T00:00:00.000Z",
        ))
        .unwrap();
    let collection = collection_for(&store, &linked);
    let first = collection.snapshot().unwrap();

    std::fs::remove_dir_all(&linked).unwrap();
    WorktreeManager::new(store.clone())
        .refresh_project(&project)
        .unwrap();
    git(&project, &["worktree", "prune"]);
    git(
        &project,
        &["worktree", "add", linked.to_str().unwrap(), "linked"],
    );
    let logs = crate::logging::capture_test_logs();

    let snapshot = collection.snapshot().unwrap();

    assert_eq!(discovery_runs(&logs, &first.projects[0]), 1);
    assert!(snapshot.projects[0].available);
    assert!(snapshot.projects[0].worktree_repository_id.is_some());
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
    archived.lifecycle = crate::storage::records::TaskLifecycle::Archived;
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
fn storage_read_failure_is_isolated_from_project_collection() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    store
        .write_task(&task_record("corrupt", "/workspace", "1"))
        .unwrap();
    drop(store);
    corrupt_last_byte(&temp.path().join("task-store-v1/tasks/corrupt/task.json"));
    let store = Store::open(temp.path().to_path_buf()).unwrap();

    let snapshot = ProjectCollectionStore::new(store).snapshot().unwrap();

    assert!(snapshot.projects.is_empty());
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

fn task_record(task_id: &str, workspace_root: &str, updated_at: &str) -> TaskRecord {
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
        updated_at: updated_at.to_string(),
        last_activity: updated_at.to_string(),
        permission_policy: Default::default(),
        composer_history: Default::default(),
        message_queue: Default::default(),
        agent_id: "agent-a".to_string(),
        agent_name: "Agent A".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace_root.to_string(),
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
