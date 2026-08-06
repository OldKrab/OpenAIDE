use openaide_app_server_protocol::ids::ProjectId;
use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;

use super::{atomic, Store};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredProjectCatalog {
    #[serde(default)]
    pub projects: Vec<StoredProject>,
    #[serde(default)]
    pub removed_project_ids: Vec<ProjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredProject {
    pub workspace_root: String,
    pub label: String,
}

impl Store {
    pub(crate) fn read_project_catalog(&self) -> Result<StoredProjectCatalog, RuntimeError> {
        let path = self.project_catalog_path();
        if !path.exists() {
            return Ok(StoredProjectCatalog::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub(crate) fn write_project_catalog(
        &self,
        catalog: &StoredProjectCatalog,
    ) -> Result<(), RuntimeError> {
        let _guard = self.lock_project_write();
        atomic::write_json(&self.project_catalog_path(), catalog)
    }

    fn project_catalog_path(&self) -> std::path::PathBuf {
        self.settings_dir().join("projects.json")
    }
}
