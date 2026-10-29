use std::time::Instant;

use base64::Engine;
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::task::{TaskImageParams, TaskImageResult};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::protocol_edge::{responses, GatewayOutcome};

use super::RpcGateway;

impl RpcGateway {
    /// Returns one stored Chat image of a Task as base64; the reference that named
    /// it carries the media type (ADR-0075).
    pub(in crate::protocol_edge) fn handle_task_image(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskImageParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let started_at = Instant::now();
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for Chat images");
        let loaded = self.task_snapshots.chat_image_for_client(
            &client.client_instance_id,
            &params.task_id,
            &params.image_id,
        );
        let fields = |outcome: &str, size_bytes: Option<usize>, error_kind: Option<String>| {
            serde_json::json!({
                "operation": "task/image",
                "task_id": params.task_id,
                "image_id": params.image_id,
                "outcome": outcome,
                "size_bytes": size_bytes,
                "duration_ms": started_at.elapsed().as_millis(),
                "error_kind": error_kind,
            })
        };
        match loaded {
            Ok(bytes) => {
                crate::logging::info(
                    "task_image_served",
                    fields("completed", Some(bytes.len()), None),
                );
                let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                self.result::<TaskImageResult>(connection_id, id, meta, TaskImageResult { data })
            }
            Err(error) => {
                let error_kind = serde_json::to_value(error.code)
                    .ok()
                    .and_then(|code| code.as_str().map(ToString::to_string));
                crate::logging::warn("task_image_failed", fields("failed", None, error_kind));
                self.error(connection_id, id, meta, error)
            }
        }
    }
}
