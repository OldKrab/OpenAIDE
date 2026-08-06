use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, ProjectId};
use std::collections::{HashMap, HashSet};
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
    store: Arc<RwLock<Option<Store>>>,
}

#[derive(Default)]
struct ProjectRootsState {
    configured_projects: Vec<ProjectTaskContext>,
    managed_projects: Vec<ProjectTaskContext>,
    client_projects: HashMap<ClientInstanceId, Vec<ProjectTaskContext>>,
    removed_project_ids: HashSet<ProjectId>,
}

impl ConfiguredProjectRoots {
    pub fn from_workspace_roots(roots: impl IntoIterator<Item = String>) -> Self {
        let configured_projects = project_contexts_from_workspace_roots(roots);
        Self {
            state: Arc::new(RwLock::new(ProjectRootsState {
                configured_projects,
                managed_projects: Vec::new(),
                client_projects: HashMap::new(),
                removed_project_ids: HashSet::new(),
            })),
            store: Arc::new(RwLock::new(None)),
        }
    }

    /// Loads and enables durable user-managed Project state for this registry.
    pub fn enable_persistence(
        &self,
        store: Store,
    ) -> Result<(), crate::protocol::errors::RuntimeError> {
        let catalog = store.read_project_catalog()?;
        let mut managed_projects = catalog
            .projects
            .into_iter()
            .map(|stored| {
                let identity = ProjectIdentity::from_workspace_root(&stored.workspace_root);
                ProjectTaskContext {
                    project_id: identity.project_id,
                    workspace_root: identity.workspace_root,
                    label: stored.label,
                    isolation: IsolationKind::Local,
                }
            })
            .collect::<Vec<_>>();
        sort_and_deduplicate_projects(&mut managed_projects);
        let mut state = self
            .state
            .write()
            .expect("Project Context registry lock poisoned");
        state.managed_projects = managed_projects;
        state.removed_project_ids = catalog.removed_project_ids.into_iter().collect();
        drop(state);
        *self
            .store
            .write()
            .expect("Project persistence lock poisoned") = Some(store);
        Ok(())
    }

    pub fn projects(&self) -> Vec<ProjectTaskContext> {
        let state = self
            .state
            .read()
            .expect("Project Context registry lock poisoned");
        visible_projects(&state)
    }

    /// Adds a user-managed Project without requiring Task history to exist first.
    pub fn add_project(&self, workspace_root: &str) -> Result<ProjectTaskContext, ProtocolError> {
        let workspace_root = workspace_root.trim();
        if workspace_root.is_empty() {
            return Err(invalid_project_root("Project folder is required"));
        }
        if !std::path::Path::new(workspace_root).is_dir() {
            return Err(invalid_project_root("Project folder does not exist"));
        }
        let identity = ProjectIdentity::from_workspace_root(workspace_root);
        let project = ProjectTaskContext {
            project_id: identity.project_id,
            workspace_root: identity.workspace_root,
            label: identity.label,
            isolation: IsolationKind::Local,
        };
        let mut state = self
            .state
            .write()
            .expect("Project Context registry lock poisoned");
        let mut managed_projects = state.managed_projects.clone();
        managed_projects.push(project.clone());
        sort_and_deduplicate_projects(&mut managed_projects);
        let mut removed = state.removed_project_ids.clone();
        removed.remove(&project.project_id);
        persist_project_catalog(&self.store, &managed_projects, &removed)?;
        state.managed_projects = managed_projects;
        state.removed_project_ids = removed;
        Ok(project)
    }

    pub fn rename_project(
        &self,
        project: &ProjectTaskContext,
        label: &str,
    ) -> Result<ProjectTaskContext, ProtocolError> {
        let label = label.trim();
        if label.is_empty() {
            return Err(invalid_project_root("Project name is required"));
        }
        let mut renamed = project.clone();
        renamed.label = label.to_string();
        let mut state = self
            .state
            .write()
            .expect("Project Context registry lock poisoned");
        let mut managed = state.managed_projects.clone();
        managed.retain(|candidate| candidate.project_id != project.project_id);
        managed.push(renamed.clone());
        sort_and_deduplicate_projects(&mut managed);
        persist_project_catalog(&self.store, &managed, &state.removed_project_ids)?;
        state.managed_projects = managed;
        Ok(renamed)
    }

    pub fn remove_project(&self, project_id: &ProjectId) -> Result<u64, ProtocolError> {
        let removed_task_count = self.tombstone_project_tasks(project_id)?;
        let mut state = self
            .state
            .write()
            .expect("Project Context registry lock poisoned");
        let mut managed = state.managed_projects.clone();
        managed.retain(|project| project.project_id != *project_id);
        let mut removed = state.removed_project_ids.clone();
        removed.insert(project_id.clone());
        persist_project_catalog(&self.store, &managed, &removed)?;
        state.managed_projects = managed;
        state.removed_project_ids = removed;
        Ok(removed_task_count)
    }

    pub(crate) fn is_removed(&self, project_id: &ProjectId) -> bool {
        self.state
            .read()
            .expect("Project Context registry lock poisoned")
            .removed_project_ids
            .contains(project_id)
    }

    fn tombstone_project_tasks(&self, project_id: &ProjectId) -> Result<u64, ProtocolError> {
        let binding = self
            .store
            .read()
            .expect("Project persistence lock poisoned");
        let Some(store) = binding.as_ref() else {
            return Ok(0);
        };
        let mut removed = 0_u64;
        for mut task in store
            .list_all_task_records_strict()
            .map_err(project_storage_error)?
        {
            let root = task.project_root.as_deref().unwrap_or(&task.workspace_root);
            if task.tombstoned
                || ProjectIdentity::from_workspace_root(root).project_id != *project_id
            {
                continue;
            }
            // Project removal is local-only: retain the native Agent session id.
            task.tombstoned = true;
            task.revision = task.revision.saturating_add(1);
            store.write_task(&task).map_err(project_storage_error)?;
            removed = removed.saturating_add(1);
        }
        Ok(removed)
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
    projects.extend(state.managed_projects.clone());
    projects.extend(state.client_projects.values().flatten().cloned());
    projects.retain(|project| !state.removed_project_ids.contains(&project.project_id));
    sort_and_deduplicate_projects(&mut projects);
    projects
}

fn persist_project_catalog(
    store: &Arc<RwLock<Option<Store>>>,
    managed: &[ProjectTaskContext],
    removed: &HashSet<ProjectId>,
) -> Result<(), ProtocolError> {
    let binding = store.read().expect("Project persistence lock poisoned");
    let Some(store) = binding.as_ref() else {
        return Ok(());
    };
    store
        .write_project_catalog(&crate::storage::projects::StoredProjectCatalog {
            projects: managed
                .iter()
                .map(|project| crate::storage::projects::StoredProject {
                    workspace_root: project.workspace_root.clone(),
                    label: project.label.clone(),
                })
                .collect(),
            removed_project_ids: removed.iter().cloned().collect(),
        })
        .map_err(project_storage_error)
}

fn invalid_project_root(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InvalidRequest,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}

fn project_storage_error(error: impl std::fmt::Display) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Internal,
        message: format!("Failed to save Project: {error}"),
        recoverable: true,
        target: None,
    }
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
