use openaide_app_server_protocol::envelopes::{ErrorEnvelope, RequestMeta, ResponseMeta};
use openaide_app_server_protocol::errors::{ErrorTarget, ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::methods::CLIENT_PROBE;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};

use super::{GatewayOutcome, GatewayResponse, InboundProtocolMessage, SharedRpcGateway};
use file_upload::{AppendChunkOutcome, ChunkUploadError, ChunkUploadRegistry, ChunkUploadRequest};

mod event_streams;
mod file_upload;
pub mod listener;
mod protocol;
mod sessions;

pub use protocol::LocalHttpProtocolHandler;

const LOCAL_HTTP_CONNECTION_ID: &str = "local-http-probe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHttpResponse {
    pub status: u16,
    pub body: String,
}

pub struct LocalHttpProbeHandler {
    gateway: SharedRpcGateway,
    auth_token: String,
}

#[derive(Clone)]
pub struct LocalHttpAppHandler {
    probe: LocalHttpProbeHandler,
    protocol: LocalHttpProtocolHandler,
    uploads: ChunkUploadRegistry,
}

impl LocalHttpAppHandler {
    pub fn new(
        gateway: SharedRpcGateway,
        auth_token: impl Into<String>,
        server_id: impl Into<String>,
    ) -> Self {
        let auth_token = auth_token.into();
        Self {
            probe: LocalHttpProbeHandler::new(gateway.clone(), auth_token.clone()),
            protocol: LocalHttpProtocolHandler::new(gateway, auth_token, server_id),
            uploads: ChunkUploadRegistry::default(),
        }
    }

    pub fn handle(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        body: &str,
    ) -> LocalHttpResponse {
        match connection_id {
            Some(connection_id) => self
                .protocol
                .handle(authorization, Some(connection_id), body),
            None => self.probe.handle(authorization, body),
        }
    }

    pub(crate) fn begin_event_stream(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
    ) -> Result<event_streams::EventStreamLease, LocalHttpResponse> {
        self.protocol
            .begin_event_stream(authorization, connection_id)
    }

    pub(crate) fn poll_session(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        session_id: &str,
        after: u64,
    ) -> LocalHttpResponse {
        self.protocol
            .poll_session(authorization, connection_id, session_id, after)
    }

    pub(crate) fn event_stream_is_current(&self, lease: &event_streams::EventStreamLease) -> bool {
        self.protocol.event_stream_is_current(lease)
    }

    pub(crate) fn observe_event_stream_activity(
        &self,
        lease: &event_streams::EventStreamLease,
    ) -> bool {
        self.protocol.observe_event_stream_activity(lease)
    }

    pub(crate) fn finish_event_stream(&self, lease: &event_streams::EventStreamLease) {
        self.protocol.finish_event_stream(lease);
    }

    pub(crate) fn drain_push_messages(&self, lease: &event_streams::EventStreamLease) -> String {
        self.protocol.drain_push_messages(lease)
    }

    pub(crate) fn authorize_upload(
        &self,
        authorization: Option<&str>,
        client_instance_id: Option<&str>,
    ) -> Result<ClientInstanceId, LocalHttpResponse> {
        match auth_status(authorization, &self.probe.auth_token) {
            AuthStatus::Authorized => {}
            AuthStatus::Missing => return Err(empty_response(401)),
            AuthStatus::Invalid => return Err(empty_response(403)),
        }
        let client_instance_id = client_instance_id
            .filter(|value| !value.trim().is_empty())
            .map(|value| ClientInstanceId::from(value.to_string()))
            .ok_or_else(|| empty_response(400))?;
        if !self
            .probe
            .gateway
            .client_is_initialized(&client_instance_id)
        {
            return Err(empty_response(409));
        }
        Ok(client_instance_id)
    }

    pub(crate) fn resolve_sent_file(
        &self,
        authorization: Option<&str>,
        client_instance_id: Option<&str>,
        task_id: Option<&str>,
        message_id: Option<&str>,
        attachment_index: Option<usize>,
    ) -> Result<crate::tasks::product_api::ResolvedSentFile, LocalHttpResponse> {
        let client_instance_id = self.authorize_upload(authorization, client_instance_id)?;
        let task_id = task_id
            .filter(|value| !value.is_empty())
            .map(|value| TaskId::from(value.to_string()))
            .ok_or_else(|| empty_response(400))?;
        let message_id = message_id
            .filter(|value| !value.is_empty())
            .ok_or_else(|| empty_response(400))?;
        let attachment_index = attachment_index.ok_or_else(|| empty_response(400))?;
        self.probe
            .gateway
            .resolve_sent_file(&client_instance_id, &task_id, message_id, attachment_index)
            .map_err(|error| match error.code {
                ProtocolErrorCode::Unauthorized => empty_response(403),
                ProtocolErrorCode::NotFound => empty_response(404),
                _ => json_response(400, json!({ "error": error })),
            })
    }

    pub(crate) fn register_uploaded_file(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: String,
        path: String,
        label: String,
    ) -> LocalHttpResponse {
        let safe_label = std::path::Path::new(&label)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("Attached file")
            .to_string();
        match self.probe.gateway.create_uploaded_file_reference(
            client_instance_id,
            TaskId::from(task_id),
            path,
            safe_label,
        ) {
            Ok(attachment) => json_response(200, json!({ "attachment": attachment })),
            Err(error) => json_response(400, json!({ "error": error })),
        }
    }

    pub(crate) fn register_uploaded_image(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: String,
        path: String,
        label: String,
        mime_type: String,
    ) -> LocalHttpResponse {
        let safe_label = std::path::Path::new(&label)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("Image")
            .to_string();
        match self.probe.gateway.create_uploaded_image(
            client_instance_id,
            TaskId::from(task_id),
            path,
            safe_label,
            mime_type,
        ) {
            Ok(attachment) => json_response(200, json!({ "attachment": attachment })),
            Err(error) => json_response(400, json!({ "error": error })),
        }
    }

    pub(crate) fn append_upload_chunk(
        &self,
        request: ChunkUploadRequest<'_>,
    ) -> Result<AppendChunkOutcome, ChunkUploadError> {
        self.uploads.append(request)
    }

    pub(crate) fn cancel_upload_chunk(
        &self,
        client_instance_id: &ClientInstanceId,
        upload_id: &str,
    ) -> bool {
        self.uploads.cancel(client_instance_id, upload_id)
    }
}

impl LocalHttpProbeHandler {
    pub fn new(gateway: SharedRpcGateway, auth_token: impl Into<String>) -> Self {
        Self {
            gateway,
            auth_token: auth_token.into(),
        }
    }

    pub fn handle(&self, authorization: Option<&str>, body: &str) -> LocalHttpResponse {
        let now = AppServerTime::now();
        handle_local_http_probe(authorization, &self.auth_token, body, |message| {
            self.gateway
                .handle_inbound(ConnectionId::new(LOCAL_HTTP_CONNECTION_ID), message, now)
        })
    }
}

impl Clone for LocalHttpProbeHandler {
    fn clone(&self) -> Self {
        Self {
            gateway: self.gateway.clone(),
            auth_token: self.auth_token.clone(),
        }
    }
}

fn handle_local_http_probe(
    authorization: Option<&str>,
    expected_token: &str,
    body: &str,
    dispatch: impl FnOnce(InboundProtocolMessage) -> GatewayOutcome,
) -> LocalHttpResponse {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return empty_response(401),
        AuthStatus::Invalid => return empty_response(403),
    }

    let request = match serde_json::from_str::<LocalHttpJsonRpcRequest>(body) {
        Ok(request) => request,
        Err(error) => {
            return json_response(
                400,
                jsonrpc_error(
                    Value::Null,
                    invalid_request(format!("Parse error: {error}")),
                ),
            );
        }
    };
    if request.jsonrpc != "2.0" {
        return json_response(
            400,
            jsonrpc_error(
                request.id.unwrap_or(Value::Null),
                invalid_request("jsonrpc must be 2.0"),
            ),
        );
    }
    let id = match request.id {
        Some(id @ (Value::String(_) | Value::Number(_))) => id,
        _ => {
            return json_response(
                400,
                jsonrpc_error(Value::Null, invalid_request("id must be string or number")),
            );
        }
    };
    if request.method != CLIENT_PROBE {
        return json_response(
            400,
            jsonrpc_error(
                id,
                invalid_request("LocalHttp probe handler only accepts client/probe"),
            ),
        );
    }

    match dispatch(InboundProtocolMessage::ClientRequest {
        id: gateway_id(&id),
        method: request.method,
        params: request.params.unwrap_or_else(|| json!({})),
        meta: request.meta,
    }) {
        GatewayOutcome::Respond { response, .. } => {
            json_response(200, gateway_response(id, response))
        }
        GatewayOutcome::Noop => json_response(
            500,
            jsonrpc_error(id, internal_error("client/probe produced no response")),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct LocalHttpJsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    meta: RequestMeta,
}

enum AuthStatus {
    Authorized,
    Missing,
    Invalid,
}

fn auth_status(authorization: Option<&str>, expected_token: &str) -> AuthStatus {
    let Some(value) = authorization else {
        return AuthStatus::Missing;
    };
    match value.strip_prefix("Bearer ") {
        Some(token) if token == expected_token => AuthStatus::Authorized,
        _ => AuthStatus::Invalid,
    }
}

fn gateway_response(id: Value, response: GatewayResponse) -> Value {
    match response {
        GatewayResponse::Result(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
        GatewayResponse::Error(error) => jsonrpc_error(id, *error),
    }
}

fn jsonrpc_error(id: Value, error: ErrorEnvelope) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": error,
    })
}

fn invalid_request(message: impl Into<String>) -> ErrorEnvelope {
    ErrorEnvelope::new(
        ProtocolError {
            code: ProtocolErrorCode::InvalidRequest,
            message: message.into(),
            recoverable: false,
            target: Some(ErrorTarget {
                method: None,
                field: None,
                current_task: None,
            }),
        },
        ResponseMeta::default(),
    )
}

fn internal_error(message: impl Into<String>) -> ErrorEnvelope {
    ErrorEnvelope::new(
        ProtocolError {
            code: ProtocolErrorCode::Internal,
            message: message.into(),
            recoverable: true,
            target: None,
        },
        ResponseMeta::default(),
    )
}

fn gateway_id(id: &Value) -> String {
    match id {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn empty_response(status: u16) -> LocalHttpResponse {
    LocalHttpResponse {
        status,
        body: String::new(),
    }
}

fn json_response(status: u16, value: Value) -> LocalHttpResponse {
    LocalHttpResponse {
        status,
        body: value.to_string(),
    }
}

#[cfg(test)]
mod tests;
