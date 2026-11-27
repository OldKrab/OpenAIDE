use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ProjectId, WorktreeRepositoryId};
use openaide_app_server_protocol::worktree::{
    WorktreeBaseSelection, WorktreeCreateParams, WorktreeCreateResult, WorktreeLinkedTasksParams,
    WorktreeLinkedTasksResult, WorktreeRecreateParams, WorktreeRecreateResult,
    WorktreeRefreshParams, WorktreeRefreshResult, WorktreeRemovalPreflightParams,
    WorktreeRemovalPreflightResult, WorktreeRemoveParams, WorktreeRemoveResult,
    WorktreeRenameParams, WorktreeRenameResult, WorktreeResolveFolderParams,
    WorktreeResolveFolderResult,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::global_state::GlobalAreas;
use crate::protocol::errors::RuntimeError;
use crate::worktrees::{CreateWorktree, RecreateWorktree, WorktreeBase};

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_worktree_refresh(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        value: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match parse::<WorktreeRefreshParams>(value) {
            Ok(params) => params,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let root = match self
            .project_root_for_worktree_request(&params.project_id, &params.repository_id)
        {
            Ok(root) => root,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        match self.worktrees.refresh_project(&root) {
            Ok(Some(result)) if result.repository.repository_id == params.repository_id => {
                // Refresh is also the recovery boundary for a configured Project whose linked
                // root was removed or recreated, so its availability projection must move too.
                // One notification carries both, and none when neither changed.
                self.publish_global_state_with(
                    GlobalAreas::PROJECTS,
                    Some(result.repository.clone()),
                );
                self.result(
                    connection_id,
                    id,
                    meta,
                    WorktreeRefreshResult {
                        repository: result.repository,
                    },
                )
            }
            Ok(_) => self.error(
                connection_id,
                id,
                meta,
                invalid("Worktree Repository does not belong to the selected Project"),
            ),
            Err(error) => self.error(connection_id, id, meta, protocol_error(error)),
        }
    }

    pub(super) fn handle_worktree_create(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        value: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match parse::<WorktreeCreateParams>(value) {
            Ok(params) => params,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let root = match self
            .project_root_for_worktree_request(&params.project_id, &params.repository_id)
        {
            Ok(root) => root,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let request = CreateWorktree {
            repository_id: params.repository_id,
            source_project_root: root,
            name: params.name,
            base: base(params.base),
            branch: params.branch,
        };
        match self.worktrees.start_create(request) {
            Ok(started) => {
                self.publish_worktree_repository(started.repository.clone());
                self.result(
                    connection_id,
                    id,
                    meta,
                    WorktreeCreateResult {
                        operation_id: started.operation_id,
                        repository: started.repository,
                    },
                )
            }
            Err(error) => self.error(connection_id, id, meta, protocol_error(error)),
        }
    }

    pub(super) fn handle_worktree_recreate(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        value: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match parse::<WorktreeRecreateParams>(value) {
            Ok(params) => params,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let root = match self
            .project_root_for_worktree_request(&params.project_id, &params.repository_id)
        {
            Ok(root) => root,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let request = RecreateWorktree {
            repository_id: params.repository_id,
            source_project_root: root,
            worktree_id: params.worktree_id,
            base: base(params.base),
            branch: params.branch,
        };
        match self.worktrees.start_recreate(request) {
            Ok(started) => {
                self.publish_worktree_repository(started.repository.clone());
                self.result(
                    connection_id,
                    id,
                    meta,
                    WorktreeRecreateResult {
                        operation_id: started.operation_id,
                        repository: started.repository,
                    },
                )
            }
            Err(error) => self.error(connection_id, id, meta, protocol_error(error)),
        }
    }

    pub(super) fn handle_worktree_removal_preflight(
        &self,
        connection_id: ConnectionId,
        id: String,
        value: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match parse::<WorktreeRemovalPreflightParams>(value) {
            Ok(params) => params,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        match self
            .worktrees
            .removal_preflight(&params.repository_id, &params.worktree_id)
        {
            Ok(preflight) => self.result(
                connection_id,
                id,
                meta,
                WorktreeRemovalPreflightResult { preflight },
            ),
            Err(error) => self.error(connection_id, id, meta, protocol_error(error)),
        }
    }

    pub(super) fn handle_worktree_remove(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        value: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match parse::<WorktreeRemoveParams>(value) {
            Ok(params) => params,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        match self
            .worktrees
            .start_remove(params.repository_id, params.worktree_id)
        {
            Ok(started) => {
                self.publish_worktree_repository(started.repository.clone());
                self.result(
                    connection_id,
                    id,
                    meta,
                    WorktreeRemoveResult {
                        operation_id: started.operation_id,
                        repository: started.repository,
                    },
                )
            }
            Err(error) => self.error(connection_id, id, meta, protocol_error(error)),
        }
    }

    pub(super) fn handle_worktree_linked_tasks(
        &self,
        connection_id: ConnectionId,
        id: String,
        value: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match parse::<WorktreeLinkedTasksParams>(value) {
            Ok(params) => params,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        match self
            .worktrees
            .linked_task_ids(&params.repository_id, &params.worktree_id)
        {
            Ok(task_ids) => self.result(
                connection_id,
                id,
                meta,
                WorktreeLinkedTasksResult { task_ids },
            ),
            Err(error) => self.error(connection_id, id, meta, protocol_error(error)),
        }
    }

    pub(super) fn handle_worktree_rename(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        value: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match parse::<WorktreeRenameParams>(value) {
            Ok(params) => params,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        match self
            .worktrees
            .rename(&params.repository_id, &params.worktree_id, &params.name)
        {
            Ok(repository) => {
                self.publish_worktree_repository(repository.clone());
                self.result(connection_id, id, meta, WorktreeRenameResult { repository })
            }
            Err(error) => self.error(connection_id, id, meta, protocol_error(error)),
        }
    }

    pub(super) fn handle_worktree_resolve_folder(
        &self,
        connection_id: ConnectionId,
        id: String,
        value: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match parse::<WorktreeResolveFolderParams>(value) {
            Ok(params) => params,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        match self
            .worktrees
            .resolve_folder(&params.repository_id, &params.worktree_id)
        {
            Ok(path) => self.result(
                connection_id,
                id,
                meta,
                WorktreeResolveFolderResult {
                    path: path.to_string_lossy().to_string(),
                },
            ),
            Err(error) => self.error(connection_id, id, meta, protocol_error(error)),
        }
    }

    pub(crate) fn publish_background_worktree_repository_update(
        &mut self,
        repository: openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot,
    ) {
        self.publish_worktree_repository(repository);
    }

    /// Resolves Current HEAD relative to the selected Project, which may itself be a linked
    /// worktree. Repository identity prevents a Project id from authorizing another repository.
    fn project_root_for_worktree_request(
        &self,
        project_id: &ProjectId,
        repository_id: &WorktreeRepositoryId,
    ) -> Result<std::path::PathBuf, ProtocolError> {
        let projects = self.snapshots.project_collection_snapshot()?;
        let project = projects
            .projects
            .into_iter()
            .find(|project| project.project_id == *project_id)
            .ok_or_else(|| invalid("Selected Project is unavailable"))?;
        if project.worktree_repository_id.as_ref() != Some(repository_id) {
            return Err(invalid(
                "Worktree Repository does not belong to the selected Project",
            ));
        }
        let project_root = std::path::PathBuf::from(project.workspace_root);
        if project_root.is_dir() {
            return Ok(project_root);
        }
        self.worktrees
            .source_project_root(repository_id)
            .map_err(protocol_error)
    }
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T, ProtocolError> {
    serde_json::from_value(value).map_err(responses::invalid_params)
}

fn base(selection: WorktreeBaseSelection) -> WorktreeBase {
    match selection {
        WorktreeBaseSelection::CurrentHead => WorktreeBase::CurrentHead,
        WorktreeBaseSelection::LocalBranch { name } => WorktreeBase::LocalBranch(name),
    }
}

fn invalid(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InvalidRequest,
        message: message.to_string(),
        recoverable: false,
        target: None,
    }
}

fn protocol_error(error: RuntimeError) -> ProtocolError {
    let code = match &error {
        RuntimeError::InvalidParams(_) => ProtocolErrorCode::InvalidRequest,
        RuntimeError::TaskNotFound(_) | RuntimeError::NativeSessionMissing(_) => {
            ProtocolErrorCode::NotFound
        }
        RuntimeError::Conflict(_) | RuntimeError::OutcomeUnknown(_) => ProtocolErrorCode::Conflict,
        RuntimeError::CapabilityMissing(_) | RuntimeError::Unsupported(_) => {
            ProtocolErrorCode::CapabilityUnavailable
        }
        RuntimeError::AuthRequired(_) => ProtocolErrorCode::Unauthorized,
        RuntimeError::NotReady(_)
        | RuntimeError::SetupRequired(_)
        | RuntimeError::NodeJsRequired(_) => ProtocolErrorCode::Conflict,
        RuntimeError::MethodNotFound(_)
        | RuntimeError::Storage(_)
        | RuntimeError::Internal(_)
        | RuntimeError::AgentReported(_) => ProtocolErrorCode::Internal,
    };
    ProtocolError {
        code,
        message: error.to_string(),
        recoverable: !matches!(code, ProtocolErrorCode::InvalidRequest),
        target: None,
    }
}
