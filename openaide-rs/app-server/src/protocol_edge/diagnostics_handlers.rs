use openaide_app_server_protocol::diagnostics::{
    RuntimeDiagnosticsParams, RuntimeDiagnosticsResult, SupportExportCreateParams,
    SupportExportCreateResult, SupportExportListParams, SupportExportListResult,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::logging;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_diagnostics_get_runtime(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<RuntimeDiagnosticsParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        let result = match self.diagnostics.runtime_diagnostics() {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<RuntimeDiagnosticsResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_diagnostics_list_support_export(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<SupportExportListParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.diagnostics.list_support_export(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<SupportExportListResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_diagnostics_create_support_export(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<SupportExportCreateParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let started_at = Instant::now();
        logging::info(
            "support_export_create_started",
            serde_json::json!({
                "session_count": params.sessions.len(),
                "unbound_trace_count": params.unbound_trace_ids.len(),
                "include_runtime_snapshot": params.include_runtime_snapshot,
                "include_logs": params.include_logs,
            }),
        );
        let Some(client) = self.client_hub.context_for_connection(&connection_id) else {
            log_support_export_completed(&started_at, "client_unavailable", None, None);
            return self.error(
                connection_id,
                id,
                meta,
                diagnostics_error(
                    ProtocolErrorCode::CapabilityUnavailable,
                    "client is unavailable",
                ),
            );
        };
        let export = match self.diagnostics.create_support_export(&params) {
            Ok(export) => export,
            Err(error) => {
                log_support_export_completed(&started_at, "export_failed", None, None);
                return self.error(connection_id, id, meta, error);
            }
        };
        let size_bytes = export.size_bytes;
        let contains_sensitive_data = export.contains_sensitive_data;
        let handle = match self.shell_file_reveals.register_local_file_for_client(
            client.client_instance_id,
            export.path,
            Some(export.label.clone()),
        ) {
            Ok(handle) => handle,
            Err(_) => {
                log_support_export_completed(
                    &started_at,
                    "handle_registration_failed",
                    Some(size_bytes),
                    Some(contains_sensitive_data),
                );
                return self.error(
                    connection_id,
                    id,
                    meta,
                    diagnostics_error(
                        ProtocolErrorCode::Internal,
                        "support export could not be registered",
                    ),
                );
            }
        };
        log_support_export_completed(
            &started_at,
            "ready",
            Some(size_bytes),
            Some(contains_sensitive_data),
        );
        self.result::<SupportExportCreateResult>(
            connection_id,
            id,
            meta,
            SupportExportCreateResult {
                file_handle_id: handle.id,
                label: export.label,
                size_bytes: export.size_bytes,
                contains_sensitive_data: export.contains_sensitive_data,
            },
        )
    }
}

fn log_support_export_completed(
    started_at: &Instant,
    outcome: &str,
    size_bytes: Option<u64>,
    contains_sensitive_data: Option<bool>,
) {
    logging::info(
        "support_export_create_completed",
        serde_json::json!({
            "outcome": outcome,
            "duration_ms": started_at.elapsed().as_millis(),
            "size_bytes": size_bytes,
            "contains_sensitive_data": contains_sensitive_data,
        }),
    );
}

fn diagnostics_error(code: ProtocolErrorCode, message: &str) -> ProtocolError {
    ProtocolError {
        code,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}
use std::time::Instant;
