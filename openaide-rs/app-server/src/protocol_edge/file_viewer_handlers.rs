use std::time::Instant;

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::file_viewer::{
    FileViewerOpenFromHandleParams, FileViewerOpenParams, FileViewerRefreshParams,
    FileViewerReleaseParams, FileViewerReleaseResult, FileViewerSnapshot,
};
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::methods::{
    FILE_VIEWER_OPEN, FILE_VIEWER_OPEN_FROM_HANDLE, FILE_VIEWER_REFRESH,
};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::file_viewer::FileViewerRegistry;
use crate::logging;

use super::{responses, GatewayOutcome, RpcGateway};

/// Authorization and workspace resolution happen under the protocol lock; filesystem and
/// image work run afterward. This job owns no gateway reference or durable Task mutation.
pub(super) struct PreparedFileViewerRead {
    pub owner: ClientInstanceId,
    registry: FileViewerRegistry,
    kind: FileViewerReadKind,
}

enum FileViewerReadKind {
    Open {
        workspace: String,
        params: FileViewerOpenParams,
    },
    FromHandle(FileViewerOpenFromHandleParams),
    Refresh(FileViewerRefreshParams),
}

impl PreparedFileViewerRead {
    pub fn run(&self, request_id: &str) -> FileViewerSnapshot {
        let started = Instant::now();
        let operation = match &self.kind {
            FileViewerReadKind::Open { .. } => "file_viewer_open",
            FileViewerReadKind::FromHandle(_) => "file_viewer_open_from_handle",
            FileViewerReadKind::Refresh(_) => "file_viewer_refresh",
        };
        logging::info(
            &format!("{operation}_started"),
            serde_json::json!({
                "request_id": request_id, "client_id": self.owner, "attempt": 1,
            }),
        );
        let snapshot = match &self.kind {
            FileViewerReadKind::Open { workspace, params } => {
                self.registry
                    .open(&self.owner, workspace, &params.path, params.line)
            }
            FileViewerReadKind::FromHandle(params) => {
                self.registry
                    .open_from_handle(&self.owner, &params.handle, &params.href)
            }
            FileViewerReadKind::Refresh(params) => {
                self.registry
                    .refresh(&self.owner, &params.handle, params.line)
            }
        };
        logging::info(
            &format!("{operation}_completed"),
            serde_json::json!({
                "request_id": request_id, "client_id": self.owner, "attempt": 1,
                "handle": snapshot.handle, "outcome": snapshot.kind,
                "error_kind": snapshot.error, "truncated": snapshot.truncated,
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        snapshot
    }
}

impl RpcGateway {
    pub(super) fn prepare_file_viewer_read(
        &self,
        connection_id: &ConnectionId,
        method: &str,
        params: Value,
    ) -> Result<PreparedFileViewerRead, ProtocolError> {
        let client = self
            .client_hub
            .context_for_connection(connection_id)
            .ok_or_else(|| responses::not_initialized(method.to_string()))?;
        let kind = match method {
            FILE_VIEWER_OPEN => {
                let params: FileViewerOpenParams =
                    serde_json::from_value(params).map_err(responses::invalid_params)?;
                let workspace = self
                    .task_snapshots
                    .workspace_root_for_client(&client.client_instance_id, &params.task_id)?;
                FileViewerReadKind::Open { workspace, params }
            }
            FILE_VIEWER_OPEN_FROM_HANDLE => FileViewerReadKind::FromHandle(
                serde_json::from_value(params).map_err(responses::invalid_params)?,
            ),
            FILE_VIEWER_REFRESH => FileViewerReadKind::Refresh(
                serde_json::from_value(params).map_err(responses::invalid_params)?,
            ),
            _ => return Err(responses::unsupported_method(method)),
        };
        Ok(PreparedFileViewerRead {
            owner: client.client_instance_id,
            registry: self.file_viewer.clone(),
            kind,
        })
    }

    pub(super) fn handle_file_viewer_open(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        self.handle_file_viewer_read(connection_id, id, FILE_VIEWER_OPEN, params, meta)
    }

    pub(super) fn handle_file_viewer_open_from_handle(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        self.handle_file_viewer_read(
            connection_id,
            id,
            FILE_VIEWER_OPEN_FROM_HANDLE,
            params,
            meta,
        )
    }

    pub(super) fn handle_file_viewer_refresh(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        self.handle_file_viewer_read(connection_id, id, FILE_VIEWER_REFRESH, params, meta)
    }

    fn handle_file_viewer_read(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        method: &str,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        match self.prepare_file_viewer_read(&connection_id, method, params) {
            Ok(read) => self.result(connection_id, id.clone(), meta, read.run(&id)),
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    pub(super) fn handle_file_viewer_release(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<FileViewerReleaseParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for File Viewer");
        self.file_viewer
            .release(&client.client_instance_id, &params.handle);
        self.result(connection_id, id, meta, FileViewerReleaseResult {})
    }
}
