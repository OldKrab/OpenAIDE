use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use openaide_app_server::storage::Store;
use openaide_app_server::worktree_events::WorktreeUpdateNotifier;
use openaide_app_server::worktrees::{
    CreateWorktree, RecreateWorktree, WorktreeBase, WorktreeManager,
};
use openaide_app_server_protocol::worktree::{
    WorktreeAvailability, WorktreeHead, WorktreeOwnership, WorktreeRemovalBlocker,
    WorktreeRemovalStatus,
};
use tempfile::TempDir;

#[test]
fn discovers_repository_worktrees_with_stable_opaque_identity() {
    let fixture = GitFixture::new();
    fixture.add_detached_worktree("review");
    let state = TempDir::new().expect("state root");
    let store = Store::open(state.path().to_path_buf()).expect("open store");

    let first = WorktreeManager::new(store.clone())
        .refresh_project(fixture.repository())
        .expect("discover worktrees")
        .expect("supported repository");
    let second = WorktreeManager::new(store)
        .refresh_project(fixture.repository())
        .expect("rediscover worktrees")
        .expect("supported repository");

    assert_eq!(
        first.repository.repository_id,
        second.repository.repository_id
    );
    assert_eq!(first.project_worktree_id, second.project_worktree_id);
    assert_eq!(first.repository.worktrees, second.repository.worktrees);
    assert_eq!(first.repository.worktrees.len(), 2);

    let project = first
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == first.project_worktree_id)
        .expect("project worktree");
    assert_eq!(project.path, fixture.repository().to_string_lossy());
    assert_eq!(project.ownership, WorktreeOwnership::External);
    assert_eq!(project.availability, WorktreeAvailability::Available);
    assert!(matches!(&project.head, WorktreeHead::Branch { name, .. } if name == "main"));

    let linked = first
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.path.ends_with("review"))
        .expect("linked worktree");
    assert_eq!(linked.ownership, WorktreeOwnership::External);
    assert_eq!(linked.availability, WorktreeAvailability::Available);
    assert!(matches!(linked.head, WorktreeHead::Detached { .. }));
}

#[test]
fn nested_project_does_not_inherit_parent_repository_support() {
    let fixture = GitFixture::new();
    let nested = fixture.repository().join("packages/frontend");
    fs::create_dir_all(&nested).expect("nested project");
    let state = TempDir::new().expect("state root");
    let manager = WorktreeManager::new(Store::open(state.path().to_path_buf()).expect("store"));

    let discovered = manager
        .refresh_project(&nested)
        .expect("check repository support");

    assert!(discovered.is_none());
}

#[test]
fn associates_project_root_tasks_with_a_linked_project_worktree() {
    let fixture = GitFixture::new();
    fixture.add_detached_worktree("linked-project");
    let linked_path = fixture.root.path().join("linked-project");
    let state = TempDir::new().expect("state root");
    let store = Store::open(state.path().to_path_buf()).expect("store");
    let mut task = task_record("root-task", &linked_path);
    task.project_root = Some(linked_path.to_string_lossy().to_string());
    store.write_task(&task).expect("write task");
    let manager = WorktreeManager::new(store);

    let repository = manager
        .refresh_project(&linked_path)
        .expect("discover linked project")
        .expect("supported repository");
    let project_worktree = repository
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == repository.project_worktree_id)
        .expect("configured project worktree");
    let primary = repository
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.is_main)
        .expect("primary Git worktree");

    assert!(!project_worktree.is_main);
    assert_eq!(project_worktree.linked_task_count, 1);
    assert_eq!(primary.linked_task_count, 0);
    assert_eq!(
        manager
            .linked_task_ids(
                &repository.repository.repository_id,
                &project_worktree.worktree_id,
            )
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn excludes_prepared_tasks_from_linked_task_counts_and_results() {
    use openaide_app_server::storage::records::TaskLifecycle;

    let fixture = GitFixture::new();
    let state = TempDir::new().expect("state root");
    let store = Store::open(state.path().to_path_buf()).expect("store");
    let visible = task_record("visible-task", fixture.repository());
    let mut prepared = task_record("prepared-task", fixture.repository());
    prepared.lifecycle = TaskLifecycle::New { lease: None };
    store.write_task(&visible).expect("write visible task");
    store.write_task(&prepared).expect("write prepared task");
    let manager = WorktreeManager::new(store);

    let repository = manager
        .refresh_project(fixture.repository())
        .expect("discover repository")
        .expect("supported repository");
    let project_root = repository
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == repository.project_worktree_id)
        .expect("project root");

    assert_eq!(project_root.linked_task_count, 1);
    assert_eq!(
        manager
            .linked_task_ids(
                &repository.repository.repository_id,
                &repository.project_worktree_id,
            )
            .expect("list linked tasks"),
        vec![openaide_app_server_protocol::ids::TaskId::from(
            "visible-task".to_string()
        )]
    );
}

#[test]
fn creates_detached_managed_worktree_and_copies_included_ignored_files() {
    let fixture = GitFixture::new();
    fixture.write_and_commit(
        &[
            (".gitignore", ".env\ncache.bin\n"),
            (".worktreeinclude", ".env\n"),
        ],
        "configure local worktree files",
    );
    fs::write(fixture.repository().join(".env"), "LOCAL_TOKEN=fixture\n").unwrap();
    fs::write(fixture.repository().join("cache.bin"), "skip\n").unwrap();
    let state = TempDir::new().expect("state root");
    let store = Store::open(state.path().to_path_buf()).expect("open store");
    let manager = WorktreeManager::new(store.clone());
    let repository = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();

    let created = manager
        .create(CreateWorktree {
            repository_id: repository.repository.repository_id,
            source_project_root: fixture.repository().to_path_buf(),
            name: "Worktree support".to_string(),
            base: WorktreeBase::CurrentHead,
            branch: None,
        })
        .expect("create managed worktree");
    let worktree = created
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == created.worktree_id)
        .expect("created worktree");

    assert_eq!(worktree.name, "Worktree support");
    assert_eq!(worktree.ownership, WorktreeOwnership::Managed);
    assert!(matches!(worktree.head, WorktreeHead::Detached { .. }));
    assert_eq!(
        fs::read_to_string(Path::new(&worktree.path).join(".env")).unwrap(),
        "LOCAL_TOKEN=fixture\n"
    );
    assert!(!Path::new(&worktree.path).join("cache.bin").exists());
    assert!(Path::new(&worktree.path).starts_with(store.worktrees_dir()));
}

#[test]
fn renames_only_openaide_worktree_metadata() {
    let fixture = GitFixture::new();
    let state = TempDir::new().expect("state root");
    let manager =
        WorktreeManager::new(Store::open(state.path().to_path_buf()).expect("open store"));
    let repository = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();
    let created = manager
        .create(CreateWorktree {
            repository_id: repository.repository.repository_id.clone(),
            source_project_root: fixture.repository().to_path_buf(),
            name: "Old name".to_string(),
            base: WorktreeBase::CurrentHead,
            branch: Some("feature/metadata-name".to_string()),
        })
        .unwrap();
    let before = created
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == created.worktree_id)
        .unwrap()
        .clone();

    let renamed = manager
        .rename(
            &repository.repository.repository_id,
            &created.worktree_id,
            "Readable name",
        )
        .expect("rename metadata");
    let after = renamed
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == created.worktree_id)
        .unwrap();

    assert_eq!(after.name, "Readable name");
    assert_eq!(after.path, before.path);
    assert_eq!(after.head, before.head);
}

#[test]
fn resolves_only_an_available_catalog_worktree_folder() {
    let fixture = GitFixture::new();
    let state = TempDir::new().expect("state root");
    let manager =
        WorktreeManager::new(Store::open(state.path().to_path_buf()).expect("open store"));
    let repository = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();
    let created = manager
        .create(CreateWorktree {
            repository_id: repository.repository.repository_id.clone(),
            source_project_root: fixture.repository().to_path_buf(),
            name: "Reveal safely".to_string(),
            base: WorktreeBase::CurrentHead,
            branch: None,
        })
        .unwrap();
    let path = manager
        .resolve_folder(&repository.repository.repository_id, &created.worktree_id)
        .expect("resolve available folder");
    git(
        fixture.repository(),
        &["worktree", "remove", "--force", &path.to_string_lossy()],
    );
    manager.refresh_project(fixture.repository()).unwrap();

    assert!(manager
        .resolve_folder(&repository.repository.repository_id, &created.worktree_id,)
        .is_err());
}

#[test]
fn background_creation_returns_an_operation_and_publishes_completion() {
    let fixture = GitFixture::new();
    fixture.write_and_commit(
        &[(".gitignore", ".env\n"), (".worktreeinclude", ".env\n")],
        "configure worktree files",
    );
    fs::write(fixture.repository().join(".env"), "fixture\n").unwrap();
    let state = TempDir::new().expect("state root");
    let store = Store::open(state.path().to_path_buf()).expect("open store");
    let (notifier, updates) = WorktreeUpdateNotifier::channel();
    let manager = WorktreeManager::with_notifier(store, notifier);
    let repository = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();

    let started = manager
        .start_create(CreateWorktree {
            repository_id: repository.repository.repository_id,
            source_project_root: fixture.repository().to_path_buf(),
            name: "Background worktree".to_string(),
            base: WorktreeBase::CurrentHead,
            branch: None,
        })
        .expect("start creation");

    assert!(started
        .repository
        .operations
        .iter()
        .any(|operation| operation.operation_id == started.operation_id));
    let completed = (0..12)
        .filter_map(|_| updates.recv_timeout(std::time::Duration::from_secs(1)).ok())
        .find(|snapshot| {
            snapshot.operations.iter().any(|operation| {
                operation.operation_id == started.operation_id
                    && operation.state
                        == openaide_app_server_protocol::worktree::WorktreeOperationState::Succeeded
            })
        })
        .expect("completed operation projection");
    let operation = completed
        .operations
        .iter()
        .find(|operation| operation.operation_id == started.operation_id)
        .unwrap();
    let created_id = operation.worktree_id.as_ref().expect("created worktree id");
    assert_eq!(operation.completed_files, Some(1));
    assert_eq!(operation.total_files, Some(1));
    assert_eq!(operation.completed_bytes, Some(8));
    assert_eq!(operation.total_bytes, Some(8));
    assert!(completed.worktrees.iter().any(|worktree| {
        &worktree.worktree_id == created_id
            && worktree.availability == WorktreeAvailability::Available
    }));
}

#[test]
fn removal_rechecks_changes_and_deletes_only_a_safe_worktree() {
    let fixture = GitFixture::new();
    let state = TempDir::new().expect("state root");
    let store = Store::open(state.path().to_path_buf()).expect("open store");
    let manager = WorktreeManager::new(store);
    let repository = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();
    let created = manager
        .create(CreateWorktree {
            repository_id: repository.repository.repository_id.clone(),
            source_project_root: fixture.repository().to_path_buf(),
            name: "Disposable".to_string(),
            base: WorktreeBase::CurrentHead,
            branch: None,
        })
        .unwrap();
    let path = created
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == created.worktree_id)
        .unwrap()
        .path
        .clone();
    fs::write(Path::new(&path).join("notes.txt"), "not committed\n").unwrap();

    let blocked = manager
        .removal_preflight(&repository.repository.repository_id, &created.worktree_id)
        .unwrap();
    assert_eq!(blocked.status, WorktreeRemovalStatus::Blocked);
    assert!(blocked
        .blockers
        .contains(&WorktreeRemovalBlocker::WorkingTreeChanges));
    assert!(manager
        .remove(&repository.repository.repository_id, &created.worktree_id)
        .is_err());
    assert!(Path::new(&path).exists());

    fs::remove_file(Path::new(&path).join("notes.txt")).unwrap();
    let safe = manager
        .removal_preflight(&repository.repository.repository_id, &created.worktree_id)
        .unwrap();
    assert_eq!(safe.status, WorktreeRemovalStatus::Safe);
    let refreshed = manager
        .remove(&repository.repository.repository_id, &created.worktree_id)
        .expect("remove clean worktree");
    assert!(!Path::new(&path).exists());
    assert!(refreshed
        .worktrees
        .iter()
        .all(|worktree| worktree.worktree_id != created.worktree_id));
}

#[test]
fn removal_forgets_the_worktree_but_keeps_linked_task_history() {
    let fixture = GitFixture::new();
    let state = TempDir::new().expect("state root");
    let store = Store::open(state.path().to_path_buf()).expect("open store");
    let manager = WorktreeManager::new(store.clone());
    let repository = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();
    let created = manager
        .create(CreateWorktree {
            repository_id: repository.repository.repository_id.clone(),
            source_project_root: fixture.repository().to_path_buf(),
            name: "History workspace".to_string(),
            base: WorktreeBase::CurrentHead,
            branch: None,
        })
        .unwrap();
    let path = created
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == created.worktree_id)
        .unwrap()
        .path
        .clone();
    let mut task = task_record("linked-task", Path::new(&path));
    task.worktree_id = Some(created.worktree_id.as_str().to_string());
    task.project_root = Some(fixture.repository().to_string_lossy().to_string());
    store.write_task(&task).unwrap();

    let refreshed = manager
        .remove(&repository.repository.repository_id, &created.worktree_id)
        .expect("remove worktree");

    let unavailable = refreshed
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == created.worktree_id)
        .expect("retained worktree record");
    assert_eq!(unavailable.availability, WorktreeAvailability::Unavailable);
    assert!(unavailable.forgotten);
    assert_eq!(unavailable.linked_task_count, 1);
    assert!(!Path::new(&path).exists());
}

#[test]
fn forgets_an_already_missing_worktree_and_uses_a_new_identity_if_it_returns() {
    let fixture = GitFixture::new();
    fixture.add_detached_worktree("review");
    let state = TempDir::new().expect("state root");
    let store = Store::open(state.path().to_path_buf()).expect("store");
    let manager = WorktreeManager::new(store.clone());
    let discovered = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();
    let external = discovered
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.path.ends_with("review"))
        .unwrap()
        .clone();
    let mut task = task_record("linked-task", Path::new(&external.path));
    task.worktree_id = Some(external.worktree_id.as_str().to_string());
    task.project_root = Some(fixture.repository().to_string_lossy().to_string());
    store.write_task(&task).unwrap();
    git(
        fixture.repository(),
        &["worktree", "remove", "--force", &external.path],
    );
    manager.refresh_project(fixture.repository()).unwrap();

    let preflight = manager
        .removal_preflight(&discovered.repository.repository_id, &external.worktree_id)
        .expect("inspect missing worktree");
    assert_eq!(preflight.status, WorktreeRemovalStatus::Safe);
    let forgotten = manager
        .remove(&discovered.repository.repository_id, &external.worktree_id)
        .expect("forget missing worktree");
    assert!(
        forgotten
            .worktrees
            .iter()
            .find(|worktree| worktree.worktree_id == external.worktree_id)
            .expect("historical worktree metadata")
            .forgotten
    );
    assert_eq!(
        manager
            .linked_task_ids(&discovered.repository.repository_id, &external.worktree_id,)
            .expect("linked task history"),
        vec![openaide_app_server_protocol::ids::TaskId::from(
            "linked-task".to_string()
        )]
    );

    fixture.add_detached_worktree("review");
    let rediscovered = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();
    let current = rediscovered
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.path == external.path && !worktree.forgotten)
        .expect("returned worktree");
    assert_ne!(current.worktree_id, external.worktree_id);
}

#[test]
fn recreates_an_unregistered_external_worktree_at_its_recorded_path() {
    let fixture = GitFixture::new();
    fixture.add_detached_worktree("review");
    let state = TempDir::new().expect("state root");
    let manager =
        WorktreeManager::new(Store::open(state.path().to_path_buf()).expect("open store"));
    let discovered = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();
    let external = discovered
        .repository
        .worktrees
        .iter()
        .find(|worktree| worktree.path.ends_with("review"))
        .unwrap()
        .clone();
    git(
        fixture.repository(),
        &["worktree", "remove", "--force", &external.path],
    );
    let unavailable = manager
        .refresh_project(fixture.repository())
        .unwrap()
        .unwrap();
    assert_eq!(
        unavailable
            .repository
            .worktrees
            .iter()
            .find(|worktree| worktree.worktree_id == external.worktree_id)
            .unwrap()
            .availability,
        WorktreeAvailability::Unavailable
    );

    let recreated = manager
        .recreate(RecreateWorktree {
            repository_id: discovered.repository.repository_id,
            source_project_root: fixture.repository().to_path_buf(),
            worktree_id: external.worktree_id.clone(),
            base: WorktreeBase::CurrentHead,
            branch: None,
        })
        .expect("recreate external worktree");
    let restored = recreated
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_id == external.worktree_id)
        .unwrap();
    assert_eq!(restored.path, external.path);
    assert_eq!(restored.ownership, WorktreeOwnership::External);
    assert_eq!(restored.availability, WorktreeAvailability::Available);
}

struct GitFixture {
    root: TempDir,
    repository: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        let root = TempDir::new().expect("git fixture");
        let repository = root.path().join("repository");
        fs::create_dir_all(&repository).expect("repository directory");
        git(&repository, &["init", "-b", "main"]);
        git(&repository, &["config", "user.name", "OpenAIDE Test"]);
        git(
            &repository,
            &["config", "user.email", "test@openaide.invalid"],
        );
        fs::write(repository.join("README.md"), "fixture\n").expect("fixture file");
        git(&repository, &["add", "README.md"]);
        git(&repository, &["commit", "-m", "fixture"]);
        Self { root, repository }
    }

    fn repository(&self) -> &Path {
        &self.repository
    }

    fn add_detached_worktree(&self, name: &str) {
        let destination = self.root.path().join(name);
        let destination = destination.to_string_lossy().to_string();
        git(
            self.repository(),
            &["worktree", "add", "--detach", &destination, "HEAD"],
        );
    }

    fn write_and_commit(&self, files: &[(&str, &str)], message: &str) {
        for (path, contents) in files {
            fs::write(self.repository.join(path), contents).expect("fixture file");
            git(self.repository(), &["add", path]);
        }
        git(self.repository(), &["commit", "-m", message]);
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn task_record(
    task_id: &str,
    workspace: &Path,
) -> openaide_app_server::storage::records::TaskRecord {
    use openaide_app_server::protocol::model::{IsolationKind, TaskStatus};
    use openaide_app_server::storage::records::{TaskLifecycle, TaskTitle, TaskTitleSource};

    openaide_app_server::storage::records::TaskRecord {
        task_id: task_id.to_string(),
        title: TaskTitle::new("Task", TaskTitleSource::User),
        status: TaskStatus::Inactive,
        task_version: 0,
        message_history_version: 0,
        unread: false,
        attention: None,
        created_at: "1".to_string(),
        updated_at: "1".to_string(),
        last_activity: "1".to_string(),
        agent_name: "Codex".to_string(),
        agent_id: "codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace.to_string_lossy().to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: TaskLifecycle::Visible,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        archived: false,
        tombstoned: false,
        config_options: Default::default(),
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: openaide_app_server::storage::records::TaskPreparationRecord::Ready,
        revision: 0,
    }
}
