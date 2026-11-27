//! Terminal lifecycle requests (ADR-0071). Bytes travel on the Terminal socket, not here.

use std::path::PathBuf;

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::terminals::{
    TerminalTarget, TerminalsAttachParams, TerminalsAttachResult, TerminalsCloseParams,
    TerminalsCloseResult, TerminalsOpenParams, TerminalsOpenResult,
};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::global_state::GlobalAreas;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_terminals_open(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TerminalsOpenParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let opened = self
            .terminal_folder(&connection_id, &params.target)
            .and_then(|(project_id, folder, target_kind)| {
                self.terminals
                    .open(project_id, &folder, params.cols, params.rows, target_kind)
            });
        match opened {
            Ok(opened) => {
                self.publish_global_state(GlobalAreas::TERMINALS);
                let result = TerminalsOpenResult {
                    terminal: opened.terminal,
                    attach: opened.attach,
                };
                self.result(connection_id, id, meta, result)
            }
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    pub(super) fn handle_terminals_attach(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TerminalsAttachParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        match self.terminals.attach(&params.terminal_id) {
            Ok(attach) => self.result(connection_id, id, meta, TerminalsAttachResult { attach }),
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    pub(super) fn handle_terminals_close(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TerminalsCloseParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        match self.terminals.close(&params.terminal_id, params.force) {
            Ok(()) => {
                self.publish_global_state(GlobalAreas::TERMINALS);
                self.result(connection_id, id, meta, TerminalsCloseResult {})
            }
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    /// For status changes a shell reports from its own thread.
    pub(crate) fn publish_background_terminal_collection_update(&mut self) {
        self.publish_global_state(GlobalAreas::TERMINALS);
    }

    /// Resolves the start folder on App Server; a client never supplies a path.
    fn terminal_folder(
        &self,
        connection_id: &ConnectionId,
        target: &TerminalTarget,
    ) -> Result<(ProjectId, PathBuf, &'static str), ProtocolError> {
        match target {
            TerminalTarget::Project { project_id } => self
                .project_roots
                .projects()
                .into_iter()
                .find(|project| &project.project_id == project_id)
                .map(|project| {
                    (
                        project.project_id,
                        PathBuf::from(project.workspace_root),
                        "project",
                    )
                })
                .ok_or_else(|| ProtocolError {
                    code: ProtocolErrorCode::NotFound,
                    message: "Project is not open".to_string(),
                    recoverable: true,
                    target: None,
                }),
            TerminalTarget::Task { task_id } => {
                let client = self
                    .client_hub
                    .context_for_connection(connection_id)
                    .ok_or_else(|| {
                        responses::not_initialized(
                            openaide_app_server_protocol::methods::TERMINALS_OPEN.to_string(),
                        )
                    })?;
                let (workspace_root, project_root) = self
                    .task_snapshots
                    .terminal_folder_for_client(&client.client_instance_id, task_id)?;
                Ok((
                    crate::projects::project_id_for_workspace(&project_root),
                    PathBuf::from(workspace_root),
                    "task",
                ))
            }
        }
    }
}
