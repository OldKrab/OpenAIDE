use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::{ProjectCollectionSnapshot, ProjectSummary};

use crate::projects::{ConfiguredProjectRoots, Project, ProjectCatalog, ProjectLifecycleFilter};
use crate::storage::Store;
use crate::worktrees::{ProjectWorktreeStatus, WorktreeManager};

pub trait ProjectCollectionSnapshotSource: Send + Sync {
    fn snapshot(&self) -> Result<ProjectCollectionSnapshot, ProtocolError>;
}

#[derive(Clone)]
pub struct ProjectCollectionStore {
    catalog: ProjectCatalog,
    configured_roots: ConfiguredProjectRoots,
    store: Store,
}

impl ProjectCollectionStore {
    pub fn new(store: Store) -> Self {
        Self::new_with_configured_roots(store, ConfiguredProjectRoots::default())
    }

    pub fn new_with_configured_roots(
        store: Store,
        configured_roots: ConfiguredProjectRoots,
    ) -> Self {
        Self {
            catalog: ProjectCatalog::new(store.clone()),
            store,
            configured_roots,
        }
    }
}

impl ProjectCollectionSnapshotSource for ProjectCollectionStore {
    fn snapshot(&self) -> Result<ProjectCollectionSnapshot, ProtocolError> {
        self.catalog
            .import_legacy_roots(
                self.configured_roots
                    .projects()
                    .into_iter()
                    .map(|project| project.workspace_root),
            )
            .map_err(snapshot_read_error)?;
        let projects = self
            .catalog
            .projects(ProjectLifecycleFilter::All)
            .map_err(snapshot_read_error)?;
        let manager = WorktreeManager::new(self.store.clone());
        let projects = projects
            .into_iter()
            .map(|project| {
                let status = manager
                    .project_status(std::path::Path::new(&project.root))
                    .map_err(snapshot_read_error)?;
                Ok(project_summary(project, status))
            })
            .collect::<Result<Vec<_>, ProtocolError>>()?;
        Ok(ProjectCollectionSnapshot { projects })
    }
}

fn project_summary(project: Project, status: ProjectWorktreeStatus) -> ProjectSummary {
    ProjectSummary {
        project_id: project.project_id,
        label: project.label,
        workspace_root: project.root,
        lifecycle: project.lifecycle,
        available: status.available,
        worktree_repository_id: status.repository_id,
        project_worktree_id: status.project_worktree_id,
        worktree_error: status.discovery_error,
    }
}

fn snapshot_read_error(error: impl std::fmt::Display) -> ProtocolError {
    ProtocolError {
        code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
        message: format!("Failed to read project collection snapshot: {error}"),
        recoverable: true,
        target: None,
    }
}

#[cfg(test)]
#[path = "project_collection_tests.rs"]
mod tests;
