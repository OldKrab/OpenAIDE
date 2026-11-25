use std::time::Instant;

use base64::Engine;
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::task::{TaskImageParams, TaskImageResult, TaskImageVariant};
use serde_json::Value;

use crate::chat_image_previews::{ChatImagePreviews, ChatPreview};
use crate::client_lifecycle::ConnectionId;
use crate::protocol_edge::{responses, GatewayOutcome};

use super::RpcGateway;

/// A `task/image` request whose access check and file read are done. Making a
/// preview runs after the protocol lock is released (ADR-0084); this job owns no
/// gateway reference and cannot fail, because the original is always a valid answer.
pub(in crate::protocol_edge) struct PreparedTaskImage {
    pub owner: ClientInstanceId,
    task_id: TaskId,
    image_id: String,
    variant: TaskImageVariant,
    bytes: Vec<u8>,
    previews: ChatImagePreviews,
    started_at: Instant,
}

impl PreparedTaskImage {
    pub fn run(self) -> TaskImageResult {
        let source_bytes = self.bytes.len();
        let outcome = match self.variant {
            TaskImageVariant::Original => None,
            TaskImageVariant::Preview => Some(self.previews.preview(&self.image_id, &self.bytes)),
        };
        let (sent, media_type) = match outcome.as_ref().map(|outcome| &outcome.preview) {
            Some(ChatPreview::Reduced { media_type, bytes }) => {
                (&bytes[..], Some((*media_type).to_string()))
            }
            Some(ChatPreview::Original) | None => (&self.bytes[..], None),
        };
        let size_bytes = sent.len();
        let data = base64::engine::general_purpose::STANDARD.encode(sent);
        crate::logging::info(
            "task_image_served",
            serde_json::json!({
                "operation": "task/image",
                "task_id": self.task_id,
                "image_id": self.image_id,
                "variant": self.variant,
                "outcome": "completed",
                "source_bytes": source_bytes,
                "size_bytes": size_bytes,
                "reduced": media_type.is_some(),
                "media_type": media_type,
                "cache_hit": outcome.as_ref().map(|outcome| outcome.cache_hit),
                "gate_wait_ms": outcome.as_ref().map(|outcome| outcome.gate_wait_ms),
                "fallback_reason": outcome.as_ref().and_then(|outcome| outcome.fallback_reason),
                "duration_ms": self.started_at.elapsed().as_millis(),
            }),
        );
        TaskImageResult { data, media_type }
    }
}

impl RpcGateway {
    /// Returns one stored Chat image of a Task as base64, as stored or as a preview
    /// (ADR-0075, ADR-0084). The shared gateway runs the preview step outside the
    /// protocol lock; this path runs both steps for callers that hold the gateway.
    pub(in crate::protocol_edge) fn handle_task_image(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        match self.prepare_task_image(&connection_id, &id, params, &meta) {
            Ok(prepared) => {
                let result = prepared.run();
                self.result::<TaskImageResult>(connection_id, id, meta, result)
            }
            Err(outcome) => *outcome,
        }
    }

    /// Checks Task access and reads the stored bytes, which needs the protocol lock;
    /// an error outcome is already logged.
    pub(in crate::protocol_edge) fn prepare_task_image(
        &mut self,
        connection_id: &ConnectionId,
        id: &str,
        params: Value,
        meta: &RequestMeta,
    ) -> Result<PreparedTaskImage, Box<GatewayOutcome>> {
        let params = match serde_json::from_value::<TaskImageParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return Err(Box::new(self.error(
                    connection_id.clone(),
                    id.to_string(),
                    meta.clone(),
                    responses::invalid_params(error),
                )))
            }
        };
        let started_at = Instant::now();
        let variant = params.variant.unwrap_or(TaskImageVariant::Original);
        crate::logging::info(
            "task_image_started",
            serde_json::json!({
                "operation": "task/image",
                "task_id": params.task_id,
                "image_id": params.image_id,
                "variant": variant,
                "attempt": 1,
            }),
        );
        let client = self
            .client_hub
            .context_for_connection(connection_id)
            .expect("routing requires an initialized client for Chat images");
        match self.task_snapshots.chat_image_for_client(
            &client.client_instance_id,
            &params.task_id,
            &params.image_id,
        ) {
            Ok(bytes) => Ok(PreparedTaskImage {
                owner: client.client_instance_id,
                task_id: params.task_id,
                image_id: params.image_id,
                variant,
                bytes,
                previews: self.chat_image_previews.clone(),
                started_at,
            }),
            Err(error) => {
                let error_kind = serde_json::to_value(error.code)
                    .ok()
                    .and_then(|code| code.as_str().map(ToString::to_string));
                crate::logging::warn(
                    "task_image_failed",
                    serde_json::json!({
                        "operation": "task/image",
                        "task_id": params.task_id,
                        "image_id": params.image_id,
                        "variant": variant,
                        "outcome": "failed",
                        "duration_ms": started_at.elapsed().as_millis(),
                        "error_kind": error_kind,
                    }),
                );
                Err(Box::new(self.error(
                    connection_id.clone(),
                    id.to_string(),
                    meta.clone(),
                    error,
                )))
            }
        }
    }
}
