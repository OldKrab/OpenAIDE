use std::path::Path;

use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::snapshot::ProjectLifecycle;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::protocol::errors::RuntimeError;
use crate::storage::{atomic, Store};

use super::ProjectIdentity;

const PROJECT_CATALOG_SCHEMA_VERSION: u32 = 1;

/// A durable user work area owned by OpenAIDE rather than an attached App Shell.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub project_id: ProjectId,
    pub label: String,
    pub root: String,
    pub lifecycle: ProjectLifecycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectLifecycleFilter {
    Active,
    Removed,
    All,
}

/// Owns Project identity and persistence behind one interface shared by every App Shell.
#[derive(Clone)]
pub struct ProjectCatalog {
    store: Store,
}

impl ProjectCatalog {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub fn register_root(&self, root: &Path, label: Option<&str>) -> Result<Project, RuntimeError> {
        if !root.is_dir() {
            return Err(RuntimeError::InvalidParams(format!(
                "Project root is not an available directory: {}",
                root.display()
            )));
        }
        let root = std::fs::canonicalize(root)?.to_string_lossy().to_string();
        let label = label
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .or_else(|| {
                Path::new(&root)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Project".to_string());
        let _guard = self.store.lock_project_write();
        let mut catalog = self.read_catalog()?;
        if let Some(index) = catalog
            .projects
            .iter()
            .position(|project| project.root == root)
        {
            if catalog.projects[index].lifecycle == ProjectLifecycle::Removed {
                catalog.projects[index].lifecycle = ProjectLifecycle::Active;
                let restored = catalog.projects[index].clone();
                atomic::write_json(&self.catalog_path(), &catalog)?;
                return Ok(restored);
            }
            return Ok(catalog.projects[index].clone());
        }
        let project = Project {
            project_id: ProjectId::from(format!("project-{}", Uuid::new_v4().simple())),
            label,
            root,
            lifecycle: ProjectLifecycle::Active,
        };
        catalog.projects.push(project.clone());
        atomic::write_json(&self.catalog_path(), &catalog)?;
        Ok(project)
    }

    /// Imports roots from pre-Catalog App Shell contracts while preserving their legacy IDs.
    pub(crate) fn import_legacy_roots(
        &self,
        roots: impl IntoIterator<Item = String>,
    ) -> Result<bool, RuntimeError> {
        let identities = roots
            .into_iter()
            .filter(|root| !root.trim().is_empty())
            .map(|root| ProjectIdentity::from_workspace_root(&root))
            .collect::<Vec<_>>();
        if identities.is_empty() {
            return Ok(false);
        }
        let _guard = self.store.lock_project_write();
        let mut catalog = self.read_catalog()?;
        let mut changed = false;
        for identity in identities {
            if let Some(project) = catalog
                .projects
                .iter_mut()
                .find(|project| project.root == identity.workspace_root)
            {
                if project.lifecycle == ProjectLifecycle::Removed {
                    project.lifecycle = ProjectLifecycle::Active;
                    changed = true;
                }
                continue;
            }
            catalog.projects.push(Project {
                project_id: identity.project_id,
                label: identity.label,
                root: identity.workspace_root,
                lifecycle: ProjectLifecycle::Active,
            });
            changed = true;
        }
        if changed {
            atomic::write_json(&self.catalog_path(), &catalog)?;
        }
        Ok(changed)
    }

    pub fn remove(&self, project_id: &ProjectId) -> Result<Project, RuntimeError> {
        let _guard = self.store.lock_project_write();
        let mut catalog = self.read_catalog()?;
        let project = catalog
            .projects
            .iter_mut()
            .find(|project| project.project_id == *project_id)
            .ok_or_else(|| {
                RuntimeError::InvalidParams(format!("Project not found: {}", project_id.as_str()))
            })?;
        project.lifecycle = ProjectLifecycle::Removed;
        let removed = project.clone();
        atomic::write_json(&self.catalog_path(), &catalog)?;
        Ok(removed)
    }

    pub fn rename(&self, project_id: &ProjectId, label: &str) -> Result<Project, RuntimeError> {
        let label = label.trim();
        if label.is_empty() {
            return Err(RuntimeError::InvalidParams(
                "Project name must not be empty".to_string(),
            ));
        }
        let _guard = self.store.lock_project_write();
        let mut catalog = self.read_catalog()?;
        let project = catalog
            .projects
            .iter_mut()
            .find(|project| project.project_id == *project_id)
            .ok_or_else(|| {
                RuntimeError::InvalidParams(format!("Project not found: {}", project_id.as_str()))
            })?;
        project.label = label.to_string();
        let renamed = project.clone();
        atomic::write_json(&self.catalog_path(), &catalog)?;
        Ok(renamed)
    }

    pub fn reconnect(&self, project_id: &ProjectId, root: &Path) -> Result<Project, RuntimeError> {
        if !root.is_dir() {
            return Err(RuntimeError::InvalidParams(format!(
                "Project root is not an available directory: {}",
                root.display()
            )));
        }
        let root = std::fs::canonicalize(root)?.to_string_lossy().to_string();
        let _guard = self.store.lock_project_write();
        let mut catalog = self.read_catalog()?;
        if catalog
            .projects
            .iter()
            .any(|project| project.project_id != *project_id && project.root == root)
        {
            return Err(RuntimeError::Conflict(
                "Project root is already registered".to_string(),
            ));
        }
        let project = catalog
            .projects
            .iter_mut()
            .find(|project| project.project_id == *project_id)
            .ok_or_else(|| {
                RuntimeError::InvalidParams(format!("Project not found: {}", project_id.as_str()))
            })?;
        project.root = root;
        project.lifecycle = ProjectLifecycle::Active;
        let reconnected = project.clone();
        atomic::write_json(&self.catalog_path(), &catalog)?;
        Ok(reconnected)
    }

    pub fn projects(&self, filter: ProjectLifecycleFilter) -> Result<Vec<Project>, RuntimeError> {
        let _guard = self.store.lock_project_write();
        let mut projects = self
            .read_catalog()?
            .projects
            .into_iter()
            .filter(|project| match filter {
                ProjectLifecycleFilter::Active => project.lifecycle == ProjectLifecycle::Active,
                ProjectLifecycleFilter::Removed => project.lifecycle == ProjectLifecycle::Removed,
                ProjectLifecycleFilter::All => true,
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| left.project_id.cmp(&right.project_id))
        });
        Ok(projects)
    }

    pub fn project(&self, project_id: &ProjectId) -> Result<Option<Project>, RuntimeError> {
        let _guard = self.store.lock_project_write();
        Ok(self
            .read_catalog()?
            .projects
            .into_iter()
            .find(|project| project.project_id == *project_id))
    }

    fn read_catalog(&self) -> Result<StoredProjectCatalog, RuntimeError> {
        let path = self.catalog_path();
        if !path.exists() {
            let catalog = self.migrate_legacy_projects()?;
            atomic::write_json(&path, &catalog)?;
            return Ok(catalog);
        }
        let catalog: StoredProjectCatalog = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if catalog.version != PROJECT_CATALOG_SCHEMA_VERSION {
            return Err(RuntimeError::Storage(format!(
                "unsupported Project Catalog version: {}",
                catalog.version
            )));
        }
        Ok(catalog)
    }

    fn migrate_legacy_projects(&self) -> Result<StoredProjectCatalog, RuntimeError> {
        let mut projects = self
            .store
            .list_all_task_records()?
            .into_iter()
            .filter(|record| !record.tombstoned)
            .map(|record| {
                let identity = ProjectIdentity::from_workspace_root(
                    record
                        .project_root
                        .as_deref()
                        .unwrap_or(&record.workspace_root),
                );
                Project {
                    project_id: identity.project_id,
                    label: identity.label,
                    root: identity.workspace_root,
                    lifecycle: ProjectLifecycle::Active,
                }
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| {
            left.root
                .cmp(&right.root)
                .then_with(|| left.project_id.cmp(&right.project_id))
        });
        projects.dedup_by(|left, right| left.root == right.root);
        Ok(StoredProjectCatalog {
            version: PROJECT_CATALOG_SCHEMA_VERSION,
            projects,
        })
    }

    fn catalog_path(&self) -> std::path::PathBuf {
        self.store.root().join("projects-v1/catalog.json")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredProjectCatalog {
    version: u32,
    projects: Vec<Project>,
}

impl Default for StoredProjectCatalog {
    fn default() -> Self {
        Self {
            version: PROJECT_CATALOG_SCHEMA_VERSION,
            projects: Vec::new(),
        }
    }
}
