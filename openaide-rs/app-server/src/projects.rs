use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, ProjectId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

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
    state: Arc<RwLock<ProjectRootsState>>,
}

#[derive(Default)]
struct ProjectRootsState {
    configured_projects: Vec<ProjectTaskContext>,
    client_projects: HashMap<ClientInstanceId, Vec<ProjectTaskContext>>,
}

impl ConfiguredProjectRoots {
    pub fn from_workspace_roots(roots: impl IntoIterator<Item = String>) -> Self {
        let configured_projects = project_contexts_from_workspace_roots(roots);
        Self {
            state: Arc::new(RwLock::new(ProjectRootsState {
                configured_projects,
                client_projects: HashMap::new(),
            })),
        }
    }

    pub fn projects(&self) -> Vec<ProjectTaskContext> {
        let state = self
            .state
            .read()
            .expect("Project Context registry lock poisoned");
        visible_projects(&state)
    }

    /// Replaces one initialized client's workspace facts as a single generation.
    pub fn replace_client_workspace_roots(
        &self,
        client_instance_id: &ClientInstanceId,
        roots: impl IntoIterator<Item = String>,
    ) -> bool {
        let replacement = project_contexts_from_workspace_roots(roots);
        let mut projects = self
            .state
            .write()
            .expect("Project Context registry lock poisoned");
        let before = visible_projects(&projects);
        if replacement.is_empty() {
            projects.client_projects.remove(client_instance_id);
        } else {
            projects
                .client_projects
                .insert(client_instance_id.clone(), replacement);
        }
        before != visible_projects(&projects)
    }

    /// Removes shell context only after the client's reconnect grace has expired.
    pub fn remove_client_workspace_roots(&self, client_instance_id: &ClientInstanceId) -> bool {
        let mut projects = self
            .state
            .write()
            .expect("Project Context registry lock poisoned");
        let before = visible_projects(&projects);
        projects.client_projects.remove(client_instance_id);
        before != visible_projects(&projects)
    }

    fn resolve(&self, project_id: &ProjectId) -> Option<ProjectTaskContext> {
        let state = self
            .state
            .read()
            .expect("Project Context registry lock poisoned");
        visible_projects(&state)
            .iter()
            .find(|project| project.project_id == *project_id)
            .cloned()
    }
}

fn project_contexts_from_workspace_roots(
    roots: impl IntoIterator<Item = String>,
) -> Vec<ProjectTaskContext> {
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
    sort_and_deduplicate_projects(&mut projects);
    projects
}

fn visible_projects(state: &ProjectRootsState) -> Vec<ProjectTaskContext> {
    let mut projects = state.configured_projects.clone();
    projects.extend(state.client_projects.values().flatten().cloned());
    sort_and_deduplicate_projects(&mut projects);
    projects
}

fn sort_and_deduplicate_projects(projects: &mut Vec<ProjectTaskContext>) {
    projects.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.project_id.cmp(&right.project_id))
    });
    projects.dedup_by(|left, right| left.project_id == right.project_id);
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
                let identity = ProjectIdentity::from_workspace_root(
                    record
                        .project_root
                        .as_deref()
                        .unwrap_or(&record.workspace_root),
                );
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
