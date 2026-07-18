use std::collections::HashMap;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::{ProjectCollectionSnapshot, ProjectSummary};

use crate::projects::{ConfiguredProjectRoots, ProjectIdentity};
use crate::storage::records::TaskRecord;
use crate::storage::Store;
use crate::worktrees::{ProjectWorktreeStatus, WorktreeManager};

pub trait ProjectCollectionSnapshotSource: Send + Sync {
    fn snapshot(&self) -> Result<ProjectCollectionSnapshot, ProtocolError>;
}

#[derive(Clone)]
pub struct ProjectCollectionStore {
    store: Store,
    configured_roots: ConfiguredProjectRoots,
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
            store,
            configured_roots,
        }
    }
}

impl ProjectCollectionSnapshotSource for ProjectCollectionStore {
    fn snapshot(&self) -> Result<ProjectCollectionSnapshot, ProtocolError> {
        let identities = project_identities(
            self.store.list_tasks().map_err(snapshot_read_error)?,
            &self.configured_roots,
        );
        let manager = WorktreeManager::new(self.store.clone());
        let projects = identities
            .into_iter()
            .map(|identity| {
                let status = manager
                    .project_status(std::path::Path::new(&identity.workspace_root))
                    .map_err(snapshot_read_error)?;
                Ok(project_summary(identity, status))
            })
            .collect::<Result<Vec<_>, ProtocolError>>()?;
        Ok(ProjectCollectionSnapshot { projects })
    }
}

fn project_identities(
    records: Vec<TaskRecord>,
    configured_roots: &ConfiguredProjectRoots,
) -> Vec<ProjectIdentity> {
    let mut latest_by_workspace = HashMap::<String, TaskRecord>::new();
    for record in records {
        let identity = ProjectIdentity::from_workspace_root(
            record
                .project_root
                .as_deref()
                .unwrap_or(&record.workspace_root),
        );
        let entry = latest_by_workspace
            .entry(identity.workspace_root)
            .or_insert_with(|| record.clone());
        if project_sort_key(&record) > project_sort_key(entry) {
            *entry = record;
        }
    }

    let mut projects = configured_roots
        .projects()
        .into_iter()
        .map(|project| ProjectIdentity::from_workspace_root(&project.workspace_root))
        .collect::<Vec<_>>();
    projects.extend(latest_by_workspace.into_values().map(|record| {
        ProjectIdentity::from_workspace_root(
            record
                .project_root
                .as_deref()
                .unwrap_or(&record.workspace_root),
        )
    }));
    projects.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.project_id.cmp(&right.project_id))
    });
    projects.dedup_by(|left, right| left.project_id == right.project_id);
    projects
}

fn project_summary(identity: ProjectIdentity, status: ProjectWorktreeStatus) -> ProjectSummary {
    ProjectSummary {
        project_id: identity.project_id,
        label: identity.label,
        workspace_root: identity.workspace_root,
        available: status.available,
        worktree_repository_id: status.repository_id,
        project_worktree_id: status.project_worktree_id,
        worktree_error: status.discovery_error,
    }
}

fn project_sort_key(record: &TaskRecord) -> (&str, &str, &str) {
    (&record.last_activity, &record.updated_at, &record.task_id)
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
