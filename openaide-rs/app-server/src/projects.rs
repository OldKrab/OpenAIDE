use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::ProjectId;

use crate::protocol::model::IsolationKind;
use crate::storage::Store;

mod identity;
pub use identity::{project_id_for_workspace, ProjectIdentity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTaskContext {
    pub project_id: ProjectId,
    pub workspace_root: String,
    pub label: String,
    pub isolation: IsolationKind,
}

#[derive(Clone, Default)]
pub struct ConfiguredProjectRoots {
    projects: Vec<ProjectTaskContext>,
}

impl ConfiguredProjectRoots {
    pub fn from_workspace_roots(roots: impl IntoIterator<Item = String>) -> Self {
        let mut projects = roots
            .into_iter()
            .filter(|root| !root.trim().is_empty())
            .map(|root| {
                let identity = ProjectIdentity::from_workspace_root(&root);
                ProjectTaskContext {
                    project_id: identity.project_id,
                    workspace_root: identity.workspace_root,
                    label: identity.label,
                    isolation: IsolationKind::Local,
                }
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| left.project_id.cmp(&right.project_id))
        });
        projects.dedup_by(|left, right| left.project_id == right.project_id);
        Self { projects }
    }

    pub fn projects(&self) -> &[ProjectTaskContext] {
        &self.projects
    }

    fn resolve(&self, project_id: &ProjectId) -> Option<ProjectTaskContext> {
        self.projects
            .iter()
            .find(|project| project.project_id == *project_id)
            .cloned()
    }
}

pub trait ProjectResolver: Send + Sync {
    fn resolve_task_context(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProjectTaskContext, ProtocolError>;
}

/// Resolves durable Project state first, then validates explicit pre-Task workspace context.
pub fn resolve_project_context(
    resolver: &dyn ProjectResolver,
    project_id: &ProjectId,
    workspace_root: Option<&str>,
) -> Result<ProjectTaskContext, ProtocolError> {
    match resolver.resolve_task_context(project_id) {
        Ok(project) => Ok(project),
        Err(error) if error.code == ProtocolErrorCode::NotFound => {
            project_context_from_workspace(project_id, workspace_root).ok_or(error)
        }
        Err(error) => Err(error),
    }
}

fn project_context_from_workspace(
    project_id: &ProjectId,
    workspace_root: Option<&str>,
) -> Option<ProjectTaskContext> {
    let workspace_root = workspace_root?.trim();
    if workspace_root.is_empty() {
        return None;
    }
    let identity = ProjectIdentity::from_workspace_root(workspace_root);
    if identity.project_id != *project_id {
        return None;
    }
    Some(ProjectTaskContext {
        project_id: identity.project_id,
        workspace_root: identity.workspace_root,
        label: identity.label,
        isolation: IsolationKind::Local,
    })
}

#[derive(Clone)]
pub struct StorageProjectResolver {
    store: Store,
    configured_roots: ConfiguredProjectRoots,
}

impl StorageProjectResolver {
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

impl ProjectResolver for StorageProjectResolver {
    fn resolve_task_context(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProjectTaskContext, ProtocolError> {
        if let Some(project) = self.configured_roots.resolve(project_id) {
            return Ok(project);
        }
        let records = self
            .store
            .list_all_task_records_strict()
            .map_err(|error| ProtocolError {
                code: ProtocolErrorCode::Internal,
                message: format!("Failed to resolve Project: {error}"),
                recoverable: true,
                target: None,
            })?;
        let mut matches = records
            .into_iter()
            .filter(|record| !record.tombstoned)
            .filter_map(|record| {
                let identity = ProjectIdentity::from_workspace_root(&record.workspace_root);
                (identity.project_id == *project_id).then_some(ProjectTaskCandidate {
                    project_id: identity.project_id,
                    label: identity.label,
                    workspace_root: identity.workspace_root,
                    isolation: record.isolation,
                    sort_key: ProjectTaskSortKey {
                        last_activity: record.last_activity,
                        updated_at: record.updated_at,
                        task_id: record.task_id,
                    },
                })
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.sort_key.cmp(&right.sort_key).reverse());
        matches
            .into_iter()
            .next()
            .map(ProjectTaskCandidate::without_sort_key)
            .ok_or_else(|| ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: format!("Project not found: {}", project_id.as_str()),
                recoverable: false,
                target: None,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProjectTaskSortKey {
    last_activity: String,
    updated_at: String,
    task_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectTaskCandidate {
    project_id: ProjectId,
    workspace_root: String,
    label: String,
    isolation: IsolationKind,
    sort_key: ProjectTaskSortKey,
}

impl ProjectTaskCandidate {
    fn without_sort_key(self) -> ProjectTaskContext {
        ProjectTaskContext {
            project_id: self.project_id,
            workspace_root: self.workspace_root,
            label: self.label,
            isolation: self.isolation,
        }
    }
}

#[cfg(test)]
mod tests;
