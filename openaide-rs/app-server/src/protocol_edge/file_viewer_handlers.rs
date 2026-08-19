use std::time::Instant;

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::file_viewer::{
    FileViewerOpenFromHandleParams, FileViewerOpenParams, FileViewerRefreshParams,
    FileViewerReleaseParams, FileViewerReleaseResult,
};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::logging;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_file_viewer_open(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let started_at = Instant::now();
        let params = match serde_json::from_value::<FileViewerOpenParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for File Viewer");
        logging::info(
            "file_viewer_open_started",
            serde_json::json!({
                "task_id": params.task_id,
                "has_line": params.line.is_some(),
            }),
        );
        let workspace_root = match self.task_snapshots.workspace_root_for_client(
            &client.client_instance_id,
            &params.task_id,
        ) {
            Ok(workspace_root) => workspace_root,
            Err(error) => {
                logging::info(
                    "file_viewer_open_completed",
                    serde_json::json!({
                        "task_id": params.task_id,
                        "outcome": "task_error",
                        "duration_ms": started_at.elapsed().as_millis(),
                    }),
                );
                return self.error(connection_id, id, meta, error);
            }
        };
        let snapshot = self.file_viewer.open(
            &client.client_instance_id,
            &workspace_root,
            &params.path,
            params.line,
        );
        logging::info(
            "file_viewer_open_completed",
            serde_json::json!({
                "task_id": params.task_id,
                "outcome": snapshot.kind,
                "truncated": snapshot.truncated,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        self.result(connection_id, id, meta, snapshot)
    }

    pub(super) fn handle_file_viewer_open_from_handle(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let started_at = Instant::now();
        let params = match serde_json::from_value::<FileViewerOpenFromHandleParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for File Viewer");
        logging::info("file_viewer_open_from_handle_started", serde_json::json!({}));
        let snapshot = self.file_viewer.open_from_handle(
            &client.client_instance_id,
            &params.handle,
            &params.href,
        );
        logging::info(
            "file_viewer_open_from_handle_completed",
            serde_json::json!({
                "outcome": snapshot.kind,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        self.result(connection_id, id, meta, snapshot)
    }

    pub(super) fn handle_file_viewer_refresh(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let started_at = Instant::now();
        let params = match serde_json::from_value::<FileViewerRefreshParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for File Viewer");
        logging::info("file_viewer_refresh_started", serde_json::json!({}));
        let snapshot =
            self.file_viewer
                .refresh(&client.client_instance_id, &params.handle, params.line);
        logging::info(
            "file_viewer_refresh_completed",
            serde_json::json!({
                "outcome": snapshot.kind,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        self.result(connection_id, id, meta, snapshot)
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
