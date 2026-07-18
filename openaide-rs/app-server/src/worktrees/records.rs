use std::fs;
use std::path::{Path, PathBuf};

use openaide_app_server_protocol::ids::{WorktreeId, WorktreeRepositoryId};
use openaide_app_server_protocol::worktree::{
    WorktreeAvailability, WorktreeHead, WorktreeOperationSnapshot, WorktreeOwnership,
    WorktreeRepositorySnapshot, WorktreeSummary,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::protocol::errors::RuntimeError;
use crate::storage::{atomic, Store};

use super::git::{GitRepositoryDiscovery, GitWorktree};
use super::ProjectWorktreeRepository;

pub(super) struct RemovalTarget {
    pub path: PathBuf,
    pub git_cwd: PathBuf,
    pub ownership: WorktreeOwnership,
    pub is_main: bool,
    pub head: WorktreeHead,
    pub available: bool,
    pub locked: bool,
    pub running_task_count: u32,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct WorktreeCatalog {
    #[serde(default)]
    repositories: Vec<RepositoryRecord>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RepositoryRecord {
    repository_id: WorktreeRepositoryId,
    common_dir: String,
    revision: u64,
    #[serde(default)]
    worktrees: Vec<WorktreeRecord>,
    #[serde(default)]
    bases: Vec<openaide_app_server_protocol::worktree::WorktreeBaseSnapshot>,
}

#[derive(Debug, Deserialize, Serialize)]
struct WorktreeRecord {
    worktree_id: WorktreeId,
    path: String,
    #[serde(default)]
    forgotten: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    ownership: WorktreeOwnership,
    is_main: bool,
    head: WorktreeHead,
    availability: WorktreeAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    availability_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locked_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prunable_reason: Option<String>,
}

impl WorktreeCatalog {
    pub fn read(store: &Store) -> Result<Self, RuntimeError> {
        let path = catalog_path(store);
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    pub fn write(&self, store: &Store) -> Result<(), RuntimeError> {
        atomic::write_json(&catalog_path(store), self)
    }

    pub fn find_worktree(&self, path: &Path) -> Option<(WorktreeRepositoryId, WorktreeId)> {
        let path = normalized_path(path);
        let find = |include_forgotten: bool| {
            self.repositories.iter().find_map(|repository| {
                repository
                    .worktrees
                    .iter()
                    .find(|worktree| {
                        (include_forgotten || !worktree.forgotten) && worktree.path == path
                    })
                    .map(|worktree| {
                        (
                            repository.repository_id.clone(),
                            worktree.worktree_id.clone(),
                        )
                    })
            })
        };

        // Prefer the live identity when a previously forgotten path is discovered again.
        // The historical identity remains a fallback so unavailable Projects keep their
        // durable repository association and linked Task history.
        find(false).or_else(|| find(true))
    }

    pub fn snapshot(
        &self,
        repository_id: &WorktreeRepositoryId,
    ) -> Option<WorktreeRepositorySnapshot> {
        self.repositories
            .iter()
            .find(|repository| &repository.repository_id == repository_id)
            .map(|repository| repository.snapshot())
    }

    pub fn synchronize(
        &mut self,
        store: &Store,
        discovery: GitRepositoryDiscovery,
    ) -> Result<ProjectWorktreeRepository, RuntimeError> {
        let common_dir = normalized_path(&discovery.common_dir);
        let repository = match self
            .repositories
            .iter_mut()
            .find(|repository| repository.common_dir == common_dir)
        {
            Some(repository) => repository,
            None => {
                self.repositories.push(RepositoryRecord {
                    repository_id: WorktreeRepositoryId::from(format!(
                        "repository-{}",
                        Uuid::new_v4()
                    )),
                    common_dir,
                    revision: 0,
                    worktrees: Vec::new(),
                    bases: Vec::new(),
                });
                self.repositories.last_mut().expect("repository inserted")
            }
        };
        synchronize_worktrees(store, repository, discovery.worktrees);
        repository.bases = discovery.bases;
        repository.revision = repository.revision.saturating_add(1);
        let project_path = normalized_path(&discovery.project_root);
        let project_worktree_id = repository
            .worktrees
            .iter()
            .find(|worktree| !worktree.forgotten && worktree.path == project_path)
            .map(|worktree| worktree.worktree_id.clone())
            .ok_or_else(|| {
                RuntimeError::Internal("Git did not list the configured Project root".to_string())
            })?;
        Ok(ProjectWorktreeRepository {
            project_worktree_id,
            repository: repository.snapshot(),
        })
    }

    pub fn reserve_managed(
        &mut self,
        store: &Store,
        repository_id: &WorktreeRepositoryId,
        name: &str,
        commit: &str,
        branch: Option<&str>,
    ) -> Result<(WorktreeId, PathBuf), RuntimeError> {
        let repository = self
            .repositories
            .iter_mut()
            .find(|repository| &repository.repository_id == repository_id)
            .ok_or_else(|| {
                RuntimeError::TaskNotFound(format!(
                    "Worktree repository {}",
                    repository_id.as_str()
                ))
            })?;
        let worktree_id = WorktreeId::from(format!("worktree-{}", Uuid::new_v4()));
        let destination = store
            .worktrees_dir()
            .join(repository_id.as_str())
            .join(worktree_id.as_str());
        let head = match branch {
            Some(name) => WorktreeHead::Branch {
                name: name.to_string(),
                commit: commit.to_string(),
            },
            None => WorktreeHead::Detached {
                commit: commit.to_string(),
            },
        };
        repository.worktrees.push(WorktreeRecord {
            worktree_id: worktree_id.clone(),
            path: normalized_path(&destination),
            forgotten: false,
            display_name: Some(name.to_string()),
            ownership: WorktreeOwnership::Managed,
            is_main: false,
            head,
            availability: WorktreeAvailability::Unavailable,
            availability_reason: Some("Worktree is being created".to_string()),
            locked_reason: None,
            prunable_reason: None,
        });
        Ok((worktree_id, destination))
    }

    pub fn remove_reserved(
        &mut self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
    ) {
        if let Some(repository) = self
            .repositories
            .iter_mut()
            .find(|repository| &repository.repository_id == repository_id)
        {
            repository
                .worktrees
                .retain(|worktree| &worktree.worktree_id != worktree_id);
        }
    }

    /// Hides a removed worktree from active inventory while retaining its Task-history metadata.
    pub fn forget(
        &mut self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
    ) -> Result<(), RuntimeError> {
        let worktree = self
            .repositories
            .iter_mut()
            .find(|repository| &repository.repository_id == repository_id)
            .and_then(|repository| {
                repository
                    .worktrees
                    .iter_mut()
                    .find(|worktree| &worktree.worktree_id == worktree_id)
            })
            .ok_or_else(|| {
                RuntimeError::TaskNotFound(format!("Worktree {}", worktree_id.as_str()))
            })?;
        worktree.forgotten = true;
        worktree.availability = WorktreeAvailability::Unavailable;
        worktree.availability_reason = Some("Workspace removed".to_string());
        worktree.locked_reason = None;
        worktree.prunable_reason = None;
        Ok(())
    }

    pub fn set_display_name(
        &mut self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
        name: String,
    ) -> Result<(), RuntimeError> {
        let worktree =
            self.repositories
                .iter_mut()
                .find(|repository| &repository.repository_id == repository_id)
                .and_then(|repository| {
                    repository.worktrees.iter_mut().find(|worktree| {
                        !worktree.forgotten && &worktree.worktree_id == worktree_id
                    })
                })
                .ok_or_else(|| {
                    RuntimeError::TaskNotFound(format!("Worktree {}", worktree_id.as_str()))
                })?;
        worktree.display_name = Some(name);
        Ok(())
    }

    pub fn removal_target(
        &self,
        repository_id: &WorktreeRepositoryId,
        worktree_id: &WorktreeId,
    ) -> Result<RemovalTarget, RuntimeError> {
        let repository = self
            .repositories
            .iter()
            .find(|repository| &repository.repository_id == repository_id)
            .ok_or_else(|| {
                RuntimeError::TaskNotFound(format!(
                    "Worktree repository {}",
                    repository_id.as_str()
                ))
            })?;
        let target = repository
            .worktrees
            .iter()
            .find(|worktree| !worktree.forgotten && &worktree.worktree_id == worktree_id)
            .ok_or_else(|| {
                RuntimeError::TaskNotFound(format!("Worktree {}", worktree_id.as_str()))
            })?;
        let git_cwd = repository
            .worktrees
            .iter()
            .find(|worktree| {
                worktree.worktree_id != target.worktree_id
                    && worktree.availability == WorktreeAvailability::Available
                    && Path::new(&worktree.path).is_dir()
            })
            .map(|worktree| PathBuf::from(&worktree.path))
            .unwrap_or_else(|| PathBuf::from(&target.path));
        Ok(RemovalTarget {
            path: PathBuf::from(&target.path),
            git_cwd,
            ownership: target.ownership,
            is_main: target.is_main,
            head: target.head.clone(),
            available: target.availability == WorktreeAvailability::Available,
            locked: target.locked_reason.is_some(),
            running_task_count: 0,
        })
    }
}

impl RepositoryRecord {
    fn snapshot(&self) -> WorktreeRepositorySnapshot {
        WorktreeRepositorySnapshot {
            repository_id: self.repository_id.clone(),
            revision: self.revision,
            worktrees: self.worktrees.iter().map(WorktreeRecord::summary).collect(),
            bases: self.bases.clone(),
            operations: Vec::<WorktreeOperationSnapshot>::new(),
        }
    }
}

impl WorktreeRecord {
    fn summary(&self) -> WorktreeSummary {
        WorktreeSummary {
            worktree_id: self.worktree_id.clone(),
            name: self
                .display_name
                .clone()
                .unwrap_or_else(|| default_name(&self.head)),
            path: self.path.clone(),
            forgotten: self.forgotten,
            ownership: self.ownership,
            is_main: self.is_main,
            head: self.head.clone(),
            availability: self.availability,
            availability_reason: self.availability_reason.clone(),
            locked_reason: self.locked_reason.clone(),
            prunable_reason: self.prunable_reason.clone(),
            project_ids: Vec::new(),
            linked_task_count: 0,
            running_task_count: 0,
            last_used_at: None,
        }
    }
}

fn synchronize_worktrees(
    store: &Store,
    repository: &mut RepositoryRecord,
    discovered: Vec<GitWorktree>,
) {
    for record in &mut repository.worktrees {
        record.availability = WorktreeAvailability::Unavailable;
        record.availability_reason = Some(if record.forgotten {
            "Workspace removed".to_string()
        } else {
            "Git no longer lists this worktree".to_string()
        });
    }
    for worktree in discovered {
        let path = normalized_path(&worktree.path);
        let existing = repository
            .worktrees
            .iter_mut()
            .find(|record| !record.forgotten && record.path == path);
        if let Some(record) = existing {
            update_record(record, worktree);
            continue;
        }
        let worktree_id = WorktreeId::from(format!("worktree-{}", Uuid::new_v4()));
        let ownership = if managed_path(store, &repository.repository_id, &worktree_id) == path {
            WorktreeOwnership::Managed
        } else {
            WorktreeOwnership::External
        };
        let mut record = WorktreeRecord {
            worktree_id,
            path,
            forgotten: false,
            display_name: None,
            ownership,
            is_main: worktree.is_main,
            head: worktree.head.clone(),
            availability: WorktreeAvailability::Unavailable,
            availability_reason: None,
            locked_reason: None,
            prunable_reason: None,
        };
        update_record(&mut record, worktree);
        repository.worktrees.push(record);
    }
    repository
        .worktrees
        .sort_by_key(|worktree| (!worktree.is_main, worktree.path.clone()));
}

fn update_record(record: &mut WorktreeRecord, discovered: GitWorktree) {
    record.forgotten = false;
    record.is_main = discovered.is_main;
    record.head = discovered.head;
    record.locked_reason = discovered.locked_reason;
    record.prunable_reason = discovered.prunable_reason;
    if Path::new(&record.path).is_dir() {
        record.availability = WorktreeAvailability::Available;
        record.availability_reason = None;
    } else {
        record.availability = WorktreeAvailability::Unavailable;
        record.availability_reason = record
            .prunable_reason
            .clone()
            .or_else(|| Some("Worktree folder is unavailable".to_string()));
    }
}

fn managed_path(
    store: &Store,
    repository_id: &WorktreeRepositoryId,
    worktree_id: &WorktreeId,
) -> String {
    normalized_path(
        &store
            .worktrees_dir()
            .join(repository_id.as_str())
            .join(worktree_id.as_str()),
    )
}

fn normalized_path(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .to_string()
}

fn default_name(head: &WorktreeHead) -> String {
    match head {
        WorktreeHead::Branch { name, .. } => name.clone(),
        WorktreeHead::Detached { commit } => {
            format!("Detached at {}", commit.chars().take(7).collect::<String>())
        }
    }
}

fn catalog_path(store: &Store) -> PathBuf {
    store.worktrees_dir().join("catalog.json")
}
