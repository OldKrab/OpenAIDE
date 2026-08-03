use std::path::Path;

use crate::protocol::errors::RuntimeError;
use crate::snapshots::{ProjectCollectionSnapshotSource, ProjectCollectionStore};
use crate::storage::Store;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::project::{
    ProjectMutationResult, ProjectReconnectParams, ProjectRegisterParams, ProjectRemoveParams,
    ProjectRenameParams,
};

use super::ProjectCatalog;

pub(crate) trait ProjectManagementWorkflow: Send + Sync {
    fn register_project(
        &self,
        params: ProjectRegisterParams,
    ) -> Result<ProjectMutationResult, ProtocolError>;
    fn rename_project(
        &self,
        params: ProjectRenameParams,
    ) -> Result<ProjectMutationResult, ProtocolError>;
    fn reconnect_project(
        &self,
        params: ProjectReconnectParams,
    ) -> Result<ProjectMutationResult, ProtocolError>;
    fn remove_project(
        &self,
        params: ProjectRemoveParams,
    ) -> Result<ProjectMutationResult, ProtocolError>;
}

pub(crate) struct ProjectManagementService {
    catalog: ProjectCatalog,
    snapshots: ProjectCollectionStore,
}

impl ProjectManagementService {
    pub(crate) fn new(store: Store) -> Self {
        Self {
            catalog: ProjectCatalog::new(store.clone()),
            snapshots: ProjectCollectionStore::new(store),
        }
    }

    fn result_for(
        &self,
        project_id: &openaide_app_server_protocol::ids::ProjectId,
    ) -> Result<ProjectMutationResult, ProtocolError> {
        let project = self
            .snapshots
            .snapshot()?
            .projects
            .into_iter()
            .find(|project| project.project_id == *project_id)
            .ok_or_else(|| project_not_found(project_id))?;
        Ok(ProjectMutationResult { project })
    }
}

impl ProjectManagementWorkflow for ProjectManagementService {
    fn register_project(
        &self,
        params: ProjectRegisterParams,
    ) -> Result<ProjectMutationResult, ProtocolError> {
        let project = self
            .catalog
            .register_root(Path::new(&params.root), params.label.as_deref())
            .map_err(protocol_error_from_runtime)?;
        self.result_for(&project.project_id)
    }

    fn rename_project(
        &self,
        params: ProjectRenameParams,
    ) -> Result<ProjectMutationResult, ProtocolError> {
        let project = self
            .catalog
            .rename(&params.project_id, &params.label)
            .map_err(protocol_error_from_runtime)?;
        self.result_for(&project.project_id)
    }

    fn reconnect_project(
        &self,
        params: ProjectReconnectParams,
    ) -> Result<ProjectMutationResult, ProtocolError> {
        let project = self
            .catalog
            .reconnect(&params.project_id, Path::new(&params.root))
            .map_err(protocol_error_from_runtime)?;
        self.result_for(&project.project_id)
    }

    fn remove_project(
        &self,
        params: ProjectRemoveParams,
    ) -> Result<ProjectMutationResult, ProtocolError> {
        let project = self
            .catalog
            .remove(&params.project_id)
            .map_err(protocol_error_from_runtime)?;
        self.result_for(&project.project_id)
    }
}

fn protocol_error_from_runtime(error: RuntimeError) -> ProtocolError {
    let code = match error {
        RuntimeError::Conflict(_) => ProtocolErrorCode::Conflict,
        RuntimeError::InvalidParams(_) => ProtocolErrorCode::ValidationFailed,
        _ => ProtocolErrorCode::Internal,
    };
    ProtocolError {
        code,
        message: error.to_string(),
        recoverable: !matches!(code, ProtocolErrorCode::ValidationFailed),
        target: None,
    }
}

fn project_not_found(project_id: &openaide_app_server_protocol::ids::ProjectId) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotFound,
        message: format!("Project not found: {}", project_id.as_str()),
        recoverable: false,
        target: None,
    }
}
