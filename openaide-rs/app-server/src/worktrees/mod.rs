//! Repository-scoped Git worktree inventory and mutation ownership.
//!
//! Callers supply a configured Project root and opaque repository/worktree ids. This module keeps
//! path authorization, Git parsing, durable identity, and operation ordering behind one interface.

mod git;
mod include_copy;
mod operations;
mod records;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::ids::{WorktreeId, WorktreeOperationId, WorktreeRepositoryId};
use openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot;
use openaide_app_server_protocol::worktree::{
    WorktreeOperationSnapshot, WorktreeRemovalBlocker, WorktreeRemovalPreflight,
    WorktreeRemovalStatus,
};

use crate::projects::ConfiguredProjectRoots;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::IsolationKind;
use crate::storage::Store;
use crate::worktree_events::WorktreeUpdateNotifier;

use self::records::WorktreeCatalog;

/// Keeps invisible prepared Tasks from retaining Native Sessions for a directory being removed.
pub(crate) trait WorktreeTaskCleanup: Send + Sync {
    fn dispose_prepared_tasks_for_worktree(
        &self,
        worktree_id: &WorktreeId,
    ) -> Result<(), RuntimeError>;
}

struct NoopWorktreeTaskCleanup;

impl WorktreeTaskCleanup for NoopWorktreeTaskCleanup {
    fn dispose_prepared_tasks_for_worktree(
        &self,
        _worktree_id: &WorktreeId,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorktreeRepository {
    pub project_worktree_id: WorktreeId,
    pub repository: WorktreeRepositorySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeBase {
    CurrentHead,
    LocalBranch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorktree {
    pub repository_id: WorktreeRepositoryId,
    pub source_project_root: PathBuf,
    pub name: String,
    pub base: WorktreeBase,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecreateWorktree {
    pub repository_id: WorktreeRepositoryId,
    pub source_project_root: PathBuf,
    pub worktree_id: WorktreeId,
    pub base: WorktreeBase,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWorktree {
    pub worktree_id: WorktreeId,
    pub repository: WorktreeRepositorySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedWorktreeOperation {
    pub operation_id: WorktreeOperationId,
    pub repository: WorktreeRepositorySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorktreeStatus {
    pub available: bool,
    pub repository_id: Option<WorktreeRepositoryId>,
    pub project_worktree_id: Option<WorktreeId>,
    pub discovery_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTaskWorkspace {
    pub path: PathBuf,
    pub worktree_id: Option<WorktreeId>,
    pub isolation: IsolationKind,
}

#[derive(Clone)]
pub struct WorktreeManager {
    store: Store,
    operations: Arc<Mutex<HashMap<WorktreeRepositoryId, Vec<WorktreeOperationSnapshot>>>>,
    operation_locks: Arc<Mutex<HashMap<WorktreeRepositoryId, Arc<Mutex<()>>>>>,
    updates: WorktreeUpdateNotifier,
    task_cleanup: Arc<dyn WorktreeTaskCleanup>,
    project_roots: ConfiguredProjectRoots,
}

impl WorktreeManager {
    pub fn new(store: Store) -> Self {
        Self::with_notifier(store, WorktreeUpdateNotifier::disabled())
    }

    pub fn with_notifier(store: Store, updates: WorktreeUpdateNotifier) -> Self {
        Self::with_notifier_and_cleanup(
            store,
            updates,
            Arc::new(NoopWorktreeTaskCleanup),
            ConfiguredProjectRoots::default(),
        )
    }

    pub(crate) fn with_notifier_and_cleanup(
        store: Store,
        updates: WorktreeUpdateNotifier,
        task_cleanup: Arc<dyn WorktreeTaskCleanup>,
        project_roots: ConfiguredProjectRoots,
    ) -> Self {
        Self {
            store,
            operations: Arc::new(Mutex::new(HashMap::new())),
            operation_locks: Arc::new(Mutex::new(HashMap::new())),
            updates,
            task_cleanup,
            project_roots,
        }
    }

    /// Resolves an opaque chooser selection to a Git-verified directory. Raw destination paths
    /// never cross the protocol mutation boundary.
    pub fn resolve_task_workspace(
        &self,
        project_root: &Path,
        worktree_id: Option<&WorktreeId>,
    ) -> Result<ResolvedTaskWorkspace, RuntimeError> {
        let Some(worktree_id) = worktree_id else {
            if !project_root.is_dir() {
                return Err(RuntimeError::Conflict(
                    "Project root is unavailable".to_string(),
                ));
            }
            return Ok(ResolvedTaskWorkspace {
                path: project_root.to_path_buf(),
                worktree_id: None,
                isolation: IsolationKind::Local,
            });
        };
        let repository = self.refresh_project(project_root)?.ok_or_else(|| {
            RuntimeError::InvalidParams("Project does not support worktrees".to_string())
        })?;
        let selected = repository
            .repository
            .worktrees
            .into_iter()
            .find(|candidate| candidate.worktree_id == *worktree_id)
            .ok_or_else(|| {
                RuntimeError::TaskNotFound(format!("Worktree {}", worktree_id.as_str()))
            })?;
        if selected.availability
            != openaide_app_server_protocol::worktree::WorktreeAvailability::Available
        {
            return Err(RuntimeError::Conflict(
                selected
                    .availability_reason
                    .unwrap_or_else(|| "Worktree is unavailable".to_string()),
            ));
        }
        Ok(ResolvedTaskWorkspace {
            path: PathBuf::from(selected.path),
            worktree_id: Some(worktree_id.clone()),
            isolation: IsolationKind::GitWorktree,
        })
    }

    /// Discovers and durably synchronizes the repository only when `project_root` is a Git top level.
    pub fn refresh_project(
        &self,
        project_root: &Path,
    ) -> Result<Option<ProjectWorktreeRepository>, RuntimeError> {
        let Some(discovery) = git::discover_project_repository(project_root)? else {
            return Ok(None);
        };
        let write = self.store.lock_worktree_write();
        let mut catalog = WorktreeCatalog::read(&self.store)?;
        let mut synchronized = catalog.synchronize(&self.store, discovery)?;
        catalog.write(&self.store)?;
        drop(write);
        self.enrich_repository(&mut synchronized.repository)?;
        self.attach_operations(&mut synchronized.repository);
        crate::logging::info(
            "worktree_repository_refreshed",
            serde_json::json!({
                "worktree_count": synchronized.repository.worktrees.len(),
                "repository_id": synchronized.repository.repository_id.as_str(),
            }),
        );
        Ok(Some(synchronized))
    }

    /// Projects remain usable when Git discovery fails; the failure disables only worktree choices.
    pub fn project_status(
        &self,
        project_root: &Path,
    ) -> Result<ProjectWorktreeStatus, RuntimeError> {
        if project_root.is_dir() {
            return match self.refresh_project(project_root) {
                Ok(Some(context)) => Ok(ProjectWorktreeStatus {
                    available: true,
                    repository_id: Some(context.repository.repository_id),
                    project_worktree_id: Some(context.project_worktree_id),
                    discovery_error: None,
                }),
                Ok(None) => Ok(ProjectWorktreeStatus {
                    available: true,
                    repository_id: None,
                    project_worktree_id: None,
                    discovery_error: None,
                }),
                Err(error) => {
                    let known = self.known_project_status(project_root)?;
                    Ok(ProjectWorktreeStatus {
                        available: true,
                        discovery_error: Some(error.to_string()),
                        ..known
                    })
                }
            };
        }
        self.known_project_status(project_root)
    }

    pub fn snapshot(
        &self,
        repository_id: &WorktreeRepositoryId,
    ) -> Result<WorktreeRepositorySnapshot, RuntimeError> {
        let read = self.store.lock_worktree_write();
        let mut repository = WorktreeCatalog::read(&self.store)?
            .snapshot(repository_id)
            .ok_or_else(|| {
                RuntimeError::TaskNotFound(format!(
                    "Worktree repository {}",
                    repository_id.as_str()
                ))
            })?;
        drop(read);
        self.enrich_repository(&mut repository)?;
        self.attach_operations(&mut repository);
        Ok(repository)
    }

    pub fn source_project_root(
        &self,
        repository_id: &WorktreeRepositoryId,
    ) -> Result<PathBuf, RuntimeError> {
        self.snapshot(repository_id)?
            .worktrees
            .into_iter()
            .find(|worktree| worktree.is_main)
            .map(|worktree| PathBuf::from(worktree.path))
            .ok_or_else(|| RuntimeError::Internal("Repository has no Project root".to_string()))
    }

    pub fn linked_task_ids(
        &self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
    ) -> Result<Vec<openaide_app_server_protocol::ids::TaskId>, RuntimeError> {
        let target_path = self
            .snapshot(repository_id)?
            .worktrees
            .into_iter()
            .find(|worktree| worktree.worktree_id == *worktree_id)
            .map(|worktree| worktree.path)
            .ok_or_else(|| {
                RuntimeError::TaskNotFound(format!("Worktree {}", worktree_id.as_str()))
            })?;
        Ok(self
            .store
            .list_all_task_records_strict()?
            .into_iter()
            .filter(|task| {
                !task.tombstoned
                    && task.lifecycle.is_visible()
                    && (task.worktree_id.as_deref() == Some(worktree_id.as_str())
                        || (task.worktree_id.is_none() && task.workspace_root == target_path))
            })
            .map(|task| openaide_app_server_protocol::ids::TaskId::from(task.task_id))
            .collect())
    }

    /// Resolves an opaque worktree identity to an available directory for a trusted host action.
    /// The webview never supplies a filesystem path for this boundary.
    pub fn resolve_folder(
        &self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
    ) -> Result<PathBuf, RuntimeError> {
        let worktree = self
            .snapshot(repository_id)?
            .worktrees
            .into_iter()
            .find(|worktree| worktree.worktree_id == *worktree_id)
            .ok_or_else(|| {
                RuntimeError::TaskNotFound(format!("Worktree {}", worktree_id.as_str()))
            })?;
        if worktree.availability
            != openaide_app_server_protocol::worktree::WorktreeAvailability::Available
        {
            return Err(RuntimeError::Conflict(
                worktree
                    .availability_reason
                    .unwrap_or_else(|| "Worktree is unavailable".to_string()),
            ));
        }
        let path = PathBuf::from(worktree.path);
        if !path.is_dir() {
            return Err(RuntimeError::Conflict(
                "Worktree folder is unavailable".to_string(),
            ));
        }
        Ok(path)
    }

    /// Changes only OpenAIDE presentation metadata; Git branch and folder identity are untouched.
    pub fn rename(
        &self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
        name: &str,
    ) -> Result<WorktreeRepositorySnapshot, RuntimeError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RuntimeError::InvalidParams(
                "Worktree name is required".to_string(),
            ));
        }
        let _write = self.store.lock_worktree_write();
        let mut catalog = WorktreeCatalog::read(&self.store)?;
        catalog.set_display_name(repository_id, worktree_id, name.chars().take(120).collect())?;
        catalog.write(&self.store)?;
        drop(_write);
        self.snapshot(repository_id)
    }

    fn enrich_repository(
        &self,
        repository: &mut WorktreeRepositorySnapshot,
    ) -> Result<(), RuntimeError> {
        let tasks = self.store.list_all_task_records_strict()?;
        for worktree in &mut repository.worktrees {
            let linked = tasks.iter().filter(|task| {
                // Prepared Tasks back New Task composers, but do not belong to visible Task history.
                if task.tombstoned || !task.lifecycle.is_visible() {
                    return false;
                }
                task.worktree_id
                    .as_deref()
                    .is_some_and(|id| id == worktree.worktree_id.as_str())
                    || (task.worktree_id.is_none() && task.workspace_root == worktree.path)
            });
            let mut linked_count = 0_u32;
            let mut running_count = 0_u32;
            let mut last_used = None::<String>;
            let mut project_ids = Vec::new();
            for task in linked {
                linked_count = linked_count.saturating_add(1);
                if matches!(
                    task.status,
                    crate::protocol::model::TaskStatus::Starting
                        | crate::protocol::model::TaskStatus::Active
                        | crate::protocol::model::TaskStatus::Stopping
                ) {
                    running_count = running_count.saturating_add(1);
                }
                if last_used
                    .as_ref()
                    .is_none_or(|current| task.last_activity > *current)
                {
                    last_used = Some(task.last_activity.clone());
                }
                let project_id = crate::projects::ProjectIdentity::from_workspace_root(
                    task.project_root.as_deref().unwrap_or(&task.workspace_root),
                )
                .project_id;
                if !project_ids.contains(&project_id) {
                    project_ids.push(project_id);
                }
            }
            worktree.linked_task_count = linked_count;
            worktree.running_task_count = running_count;
            worktree.last_used_at = last_used;
            worktree.project_ids = project_ids;
        }
        Ok(())
    }

    fn attach_operations(&self, repository: &mut WorktreeRepositorySnapshot) {
        repository.operations = self
            .operations
            .lock()
            .expect("worktree operation lock poisoned")
            .get(&repository.repository_id)
            .cloned()
            .unwrap_or_default();
    }

    pub fn create(&self, request: CreateWorktree) -> Result<CreatedWorktree, RuntimeError> {
        self.create_inner(request, None)
    }

    fn create_for_operation(
        &self,
        request: CreateWorktree,
        operation_id: &WorktreeOperationId,
    ) -> Result<CreatedWorktree, RuntimeError> {
        self.create_inner(request, Some(operation_id))
    }

    fn create_inner(
        &self,
        request: CreateWorktree,
        operation_id: Option<&WorktreeOperationId>,
    ) -> Result<CreatedWorktree, RuntimeError> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(RuntimeError::InvalidParams(
                "Worktree name is required".to_string(),
            ));
        }
        let context = self
            .refresh_project(&request.source_project_root)?
            .ok_or_else(|| {
                RuntimeError::InvalidParams("Project does not support worktrees".to_string())
            })?;
        if context.repository.repository_id != request.repository_id {
            return Err(RuntimeError::InvalidParams(
                "Worktree Repository does not belong to the selected Project".to_string(),
            ));
        }
        let base = git::resolve_base(&request.source_project_root, &request.base)?;
        if let Some(branch) = request.branch.as_deref() {
            git::validate_new_branch(&request.source_project_root, branch)?;
        }
        let (worktree_id, destination) = {
            let _write = self.store.lock_worktree_write();
            let mut catalog = WorktreeCatalog::read(&self.store)?;
            let reserved = catalog.reserve_managed(
                &self.store,
                &request.repository_id,
                name,
                &base.commit,
                request.branch.as_deref(),
            )?;
            catalog.write(&self.store)?;
            reserved
        };
        if let Err(error) = git::add_worktree(
            &request.source_project_root,
            &destination,
            &base.revision,
            request.branch.as_deref(),
        ) {
            if !git::worktree_is_registered(&request.source_project_root, &destination)? {
                let _write = self.store.lock_worktree_write();
                let mut catalog = WorktreeCatalog::read(&self.store)?;
                catalog.remove_reserved(&request.repository_id, &worktree_id);
                catalog.write(&self.store)?;
            }
            return Err(error);
        }
        let copy_result = if let Some(operation_id) = operation_id {
            include_copy::copy_included_files_with_progress(
                &request.source_project_root,
                &destination,
                |progress| {
                    self.update_copy_progress(&request.repository_id, operation_id, progress)
                },
            )
        } else {
            include_copy::copy_included_files(&request.source_project_root, &destination)
        };
        let refreshed = self
            .refresh_project(&request.source_project_root)?
            .ok_or_else(|| {
                RuntimeError::Internal("Created repository became unavailable".to_string())
            })?;
        copy_result?;
        Ok(CreatedWorktree {
            worktree_id,
            repository: refreshed.repository,
        })
    }

    pub fn removal_preflight(
        &self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
    ) -> Result<WorktreeRemovalPreflight, RuntimeError> {
        let mut target = {
            let _read = self.store.lock_worktree_write();
            WorktreeCatalog::read(&self.store)?.removal_target(repository_id, worktree_id)?
        };
        target.running_task_count = self
            .store
            .list_all_task_records_strict()?
            .into_iter()
            .filter(|task| {
                !task.tombstoned
                    && task.lifecycle.is_visible()
                    && task.worktree_id.as_deref() == Some(worktree_id.as_str())
                    && matches!(
                        task.status,
                        crate::protocol::model::TaskStatus::Starting
                            | crate::protocol::model::TaskStatus::Active
                            | crate::protocol::model::TaskStatus::Stopping
                    )
            })
            .count() as u32;
        let mut blockers = Vec::new();
        if target.is_main {
            blockers.push(WorktreeRemovalBlocker::PrimaryWorktree);
        }
        if target.running_task_count > 0 {
            blockers.push(WorktreeRemovalBlocker::RunningTasks);
        }
        if target.available && target.locked {
            blockers.push(WorktreeRemovalBlocker::Locked);
        }
        if target.available && !target.is_main {
            if git::working_tree_dirty(&target.path)? {
                blockers.push(WorktreeRemovalBlocker::WorkingTreeChanges);
            }
            if git::has_initialized_submodules(&target.path)? {
                blockers.push(WorktreeRemovalBlocker::InitializedSubmodules);
            }
            if !git::detached_head_is_preserved(&target.path, &target.head)? {
                blockers.push(WorktreeRemovalBlocker::DetachedCommits);
            }
        }
        Ok(WorktreeRemovalPreflight {
            status: if blockers.is_empty() {
                WorktreeRemovalStatus::Safe
            } else {
                WorktreeRemovalStatus::Blocked
            },
            blockers,
            ownership: target.ownership,
            path: target.path.to_string_lossy().to_string(),
            ignored_files_will_be_removed: target.available,
        })
    }

    pub fn recreate(
        &self,
        request: RecreateWorktree,
    ) -> Result<WorktreeRepositorySnapshot, RuntimeError> {
        self.recreate_inner(request, None)
    }

    fn recreate_for_operation(
        &self,
        request: RecreateWorktree,
        operation_id: &WorktreeOperationId,
    ) -> Result<WorktreeRepositorySnapshot, RuntimeError> {
        self.recreate_inner(request, Some(operation_id))
    }

    fn recreate_inner(
        &self,
        request: RecreateWorktree,
        operation_id: Option<&WorktreeOperationId>,
    ) -> Result<WorktreeRepositorySnapshot, RuntimeError> {
        let context = self
            .refresh_project(&request.source_project_root)?
            .ok_or_else(|| {
                RuntimeError::InvalidParams("Project does not support worktrees".to_string())
            })?;
        if context.repository.repository_id != request.repository_id {
            return Err(RuntimeError::InvalidParams(
                "Worktree Repository does not belong to the selected Project".to_string(),
            ));
        }
        let target = {
            let _read = self.store.lock_worktree_write();
            WorktreeCatalog::read(&self.store)?
                .removal_target(&request.repository_id, &request.worktree_id)?
        };
        if target.available {
            return Err(RuntimeError::Conflict(
                "Worktree is already available".to_string(),
            ));
        }
        let base = git::resolve_base(&request.source_project_root, &request.base)?;
        git::add_recreated_worktree(
            &request.source_project_root,
            &target.path,
            &base.revision,
            request.branch.as_deref(),
        )?;
        let copy_result = if let Some(operation_id) = operation_id {
            include_copy::copy_included_files_with_progress(
                &request.source_project_root,
                &target.path,
                |progress| {
                    self.update_copy_progress(&request.repository_id, operation_id, progress)
                },
            )
        } else {
            include_copy::copy_included_files(&request.source_project_root, &target.path)
        };
        let refreshed = self
            .refresh_project(&request.source_project_root)?
            .ok_or_else(|| RuntimeError::Internal("Repository became unavailable".to_string()))?;
        copy_result?;
        Ok(refreshed.repository)
    }

    pub fn remove(
        &self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
    ) -> Result<WorktreeRepositorySnapshot, RuntimeError> {
        let preflight = self.removal_preflight(repository_id, worktree_id)?;
        if preflight.status != WorktreeRemovalStatus::Safe {
            return Err(RuntimeError::Conflict(format!(
                "Worktree removal is blocked: {:?}",
                preflight.blockers
            )));
        }
        // Prepared Tasks never block removal, but their Native Sessions must not outlive the
        // workspace. Visible linked Tasks remain durable against forgotten historical metadata.
        self.task_cleanup
            .dispose_prepared_tasks_for_worktree(worktree_id)?;
        let target = {
            let _read = self.store.lock_worktree_write();
            WorktreeCatalog::read(&self.store)?.removal_target(repository_id, worktree_id)?
        };
        let preserve_record = self
            .store
            .list_all_task_records_strict()?
            .iter()
            .any(|task| {
                !task.tombstoned
                    && task.lifecycle.is_visible()
                    && (task.worktree_id.as_deref() == Some(worktree_id.as_str())
                        || (task.worktree_id.is_none()
                            && Path::new(&task.workspace_root) == target.path))
            })
            || self
                .project_roots
                .projects()
                .iter()
                .any(|project| Path::new(&project.workspace_root) == target.path);
        if target.available {
            git::remove_worktree(&target.git_cwd, &target.path)?;
        }
        {
            let _write = self.store.lock_worktree_write();
            let mut catalog = WorktreeCatalog::read(&self.store)?;
            if preserve_record {
                catalog.forget(repository_id, worktree_id)?;
            } else {
                catalog.remove_reserved(repository_id, worktree_id);
            }
            catalog.write(&self.store)?;
        }
        let refreshed = self
            .refresh_project(&target.git_cwd)?
            .ok_or_else(|| RuntimeError::Internal("Repository became unavailable".to_string()))?;
        Ok(refreshed.repository)
    }

    fn known_project_status(
        &self,
        project_root: &Path,
    ) -> Result<ProjectWorktreeStatus, RuntimeError> {
        let _read = self.store.lock_worktree_write();
        let catalog = WorktreeCatalog::read(&self.store)?;
        let known = catalog.find_worktree(project_root);
        Ok(ProjectWorktreeStatus {
            available: false,
            repository_id: known.as_ref().map(|known| known.0.clone()),
            project_worktree_id: known.map(|known| known.1),
            discovery_error: None,
        })
    }
}
