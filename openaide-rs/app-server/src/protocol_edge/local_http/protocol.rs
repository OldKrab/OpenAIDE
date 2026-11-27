use openaide_app_server_protocol::methods::{
    AGENT_AUTHENTICATE, CLIENT_HEARTBEAT, TASK_ATTENTION_WAIT,
};
use serde_json::{json, Value};

use crate::client_lifecycle::ConnectionId;
use crate::logging;
use crate::protocol_edge::outbox::ProtocolExchange;
use crate::protocol_edge::stdio::wire::{
    parse_inbound, parse_rejection, InboundRejection, WireMessage,
};
use crate::protocol_edge::{InboundProtocolMessage, SharedRpcGateway};

use super::attention_wait::{self, AttentionWaitTiming, HoldEnd};
use super::reliable_upload_chunks::{
    AppendOutcome as ReliableChunkOutcome, ReliableUploadChunkRegistry,
};
#[cfg(test)]
use super::session_dispatch::handle_reliable_session_upload;
use super::session_dispatch::{
    accept_reliable_session_upload, dispatch_reliable_session_upload, handle_reliable_session_open,
    handle_reliable_session_poll, is_agent_authenticate_request, reliable_chunk_error_response,
    reliable_upload_rejection,
};
use super::{auth_status, empty_response, json_response, AuthStatus, LocalHttpResponse};
use crate::protocol_edge::reliable_session::ReliableSessionRegistry;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReliableUploadChunk {
    session_id: String,
    sequence: u64,
    offset: usize,
    total_size: usize,
    data: String,
}

pub struct LocalHttpProtocolHandler {
    pub(super) gateway: SharedRpcGateway,
    pub(super) auth_token: String,
    pub(super) sessions: ReliableSessionRegistry,
    upload_chunks: ReliableUploadChunkRegistry,
    attention_wait: AttentionWaitTiming,
}

impl Clone for LocalHttpProtocolHandler {
    fn clone(&self) -> Self {
        Self {
            gateway: self.gateway.clone(),
            auth_token: self.auth_token.clone(),
            sessions: self.sessions.clone(),
            upload_chunks: self.upload_chunks.clone(),
            attention_wait: self.attention_wait,
        }
    }
}

impl LocalHttpProtocolHandler {
    pub fn new(
        gateway: SharedRpcGateway,
        auth_token: impl Into<String>,
        server_id: impl Into<String>,
    ) -> Self {
        Self {
            auth_token: auth_token.into(),
            // Session frames and gateway deliveries wake the same waiting links.
            sessions: ReliableSessionRegistry::with_delivery_signal(
                server_id,
                gateway.delivery_signal(),
            ),
            gateway,
            upload_chunks: ReliableUploadChunkRegistry::default(),
            attention_wait: AttentionWaitTiming::default(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_attention_wait(mut self, timing: AttentionWaitTiming) -> Self {
        self.attention_wait = timing;
        self
    }

    pub fn handle(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        body: &str,
    ) -> LocalHttpResponse {
        let parsed = serde_json::from_str::<Value>(body).ok();
        let transport_kind = parsed
            .as_ref()
            .and_then(|value| value.get("transport"))
            .and_then(Value::as_str)
            .unwrap_or("rpc");
        let method = parsed
            .as_ref()
            .and_then(|value| value.get("method"))
            .and_then(Value::as_str);
        let should_log = method != Some(CLIENT_HEARTBEAT);
        // A watching App Shell holds this request all day; only a failure gets a line.
        let quiet_unless_failed = method == Some(TASK_ATTENTION_WAIT);
        let started_at = std::time::Instant::now();
        let mut hold_end = None;
        if should_log && !quiet_unless_failed {
            logging::info(
                "local_http_request_started",
                serde_json::json!({
                    "connection_id": connection_id,
                    "transport": transport_kind,
                    "method": method,
                    "body_bytes": body.len(),
                }),
            );
        }
        let response = if transport_kind == "chunk" {
            self.handle_reliable_upload_chunk(authorization, connection_id, body)
        } else if transport_kind == "open" {
            handle_reliable_session_open(
                authorization,
                &self.auth_token,
                connection_id,
                &self.sessions,
            )
        } else if transport_kind == "send" {
            self.handle_reliable_session_upload(authorization, connection_id, body)
        } else {
            hold_end = self.hold_attention_wait(authorization, connection_id, parsed.as_ref());
            handle_local_http_protocol(
                authorization,
                &self.auth_token,
                connection_id,
                body,
                &self.gateway,
            )
        };
        if should_log && (!quiet_unless_failed || response.status != 200) {
            logging::info(
                "local_http_request_completed",
                serde_json::json!({
                    "connection_id": connection_id,
                    "transport": transport_kind,
                    "method": method,
                    "http_status": response.status,
                    "hold_end": hold_end.map(HoldEnd::as_str),
                    "duration_ms": started_at.elapsed().as_millis(),
                }),
            );
        }
        response
    }

    /// Holds an authenticated `taskAttention/wait` that carries a cursor until its
    /// set changes; any request that would be rejected is dispatched at once instead.
    fn hold_attention_wait(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        request: Option<&Value>,
    ) -> Option<HoldEnd> {
        let cursor = attention_wait::held_cursor(request)?;
        if !matches!(
            auth_status(authorization, &self.auth_token),
            AuthStatus::Authorized
        ) {
            return None;
        }
        let connection_id = valid_connection_id(connection_id)?;
        Some(attention_wait::hold_while_current(
            &self.gateway,
            &connection_id,
            cursor,
            self.attention_wait,
        ))
    }

    fn handle_reliable_session_upload(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        body: &str,
    ) -> LocalHttpResponse {
        let accepted = match accept_reliable_session_upload(
            authorization,
            &self.auth_token,
            connection_id,
            body,
            &self.sessions,
        ) {
            Ok(accepted) => accepted,
            Err(response) => return response,
        };
        if !is_agent_authenticate_request(&accepted.message) {
            return dispatch_reliable_session_upload(
                authorization,
                &self.auth_token,
                connection_id,
                accepted,
                &self.sessions,
                &self.gateway,
            );
        }

        // Authentication can wait indefinitely for the user. The reliable upload ACK means the
        // frame was accepted, not that its RPC completed; returning it now lets the same ordered
        // client channel deliver agent/cancelAuthenticate while this work remains in flight.
        let authorization = authorization.map(str::to_string);
        let raw_connection_id = connection_id.map(str::to_string);
        let auth_token = self.auth_token.clone();
        let gateway = self.gateway.clone();
        let sessions = self.sessions.clone();
        let session_id = accepted.session_id.clone();
        std::thread::spawn(move || {
            let started_at = std::time::Instant::now();
            logging::info(
                "local_http_deferred_request_started",
                json!({
                    "connection_id": raw_connection_id,
                    "session_id": session_id,
                    "method": AGENT_AUTHENTICATE,
                }),
            );
            let response = dispatch_reliable_session_upload(
                authorization.as_deref(),
                &auth_token,
                raw_connection_id.as_deref(),
                accepted,
                &sessions,
                &gateway,
            );
            let outcome = if response.status == 204 {
                "completed"
            } else {
                "failed"
            };
            logging::info(
                "local_http_deferred_request_completed",
                json!({
                    "connection_id": raw_connection_id,
                    "session_id": session_id,
                    "method": AGENT_AUTHENTICATE,
                    "outcome": outcome,
                    "http_status": response.status,
                    "duration_ms": started_at.elapsed().as_millis(),
                }),
            );
        });
        empty_response(204)
    }

    fn handle_reliable_upload_chunk(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        body: &str,
    ) -> LocalHttpResponse {
        match auth_status(authorization, &self.auth_token) {
            AuthStatus::Authorized => {}
            AuthStatus::Missing => return empty_response(401),
            AuthStatus::Invalid => return empty_response(403),
        }
        let raw_connection_id = connection_id;
        let Some(connection_id) = valid_connection_id(raw_connection_id) else {
            return reliable_upload_rejection("invalid_connection_id", "chunk");
        };
        let chunk = match serde_json::from_str::<ReliableUploadChunk>(body) {
            Ok(chunk) => chunk,
            Err(_) => return reliable_upload_rejection("invalid_chunk_envelope", "chunk"),
        };
        match self.sessions.connection_id(&chunk.session_id) {
            None => return empty_response(410),
            Some(owner) if owner != connection_id => return empty_response(403),
            Some(_) => {}
        }
        let bytes =
            match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, chunk.data) {
                Ok(bytes) => bytes,
                Err(_) => return reliable_upload_rejection("invalid_chunk_base64", "chunk"),
            };
        match self.upload_chunks.append(
            &chunk.session_id,
            chunk.sequence,
            chunk.offset,
            chunk.total_size,
            bytes,
        ) {
            Ok(ReliableChunkOutcome::Partial) => empty_response(202),
            Ok(ReliableChunkOutcome::Complete(upload)) => {
                self.handle(authorization, raw_connection_id, &upload)
            }
            Err(error) => reliable_chunk_error_response(error),
        }
    }

    pub fn poll_session(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        session_id: &str,
        after: u64,
    ) -> LocalHttpResponse {
        let started_at = std::time::Instant::now();
        let response = handle_reliable_session_poll(
            authorization,
            &self.auth_token,
            connection_id,
            session_id,
            after,
            &self.sessions,
            &self.gateway,
        );
        if response.status != 204 {
            logging::info(
                "local_http_reliable_poll_completed",
                serde_json::json!({
                    "connection_id": connection_id,
                    "session_id": session_id,
                    "after_sequence": after,
                    "http_status": response.status,
                    "duration_ms": started_at.elapsed().as_millis(),
                }),
            );
        }
        response
    }
}

pub(super) fn handle_local_http_protocol(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    body: &str,
    exchange: &impl ProtocolExchange,
) -> LocalHttpResponse {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return empty_response(401),
        AuthStatus::Invalid => return empty_response(403),
    }
    let Some(connection_id) = valid_connection_id(connection_id) else {
        return protocol_rejection(InboundRejection::invalid_request(
            "invalid_connection_id",
            None,
            "missing or invalid X-OpenAIDE-Connection-Id",
        ));
    };
    let message = match parse_protocol_message(body) {
        Ok(message) => message,
        Err(rejection) => return protocol_rejection(rejection),
    };
    let request_id = match &message {
        InboundProtocolMessage::ClientRequest { wire_id, .. } => Some(wire_id.clone()),
        _ => None,
    };
    let replied = exchange.dispatch(connection_id.clone(), message);
    if let (Some(id), false) = (request_id, replied) {
        return json_response(
            500,
            rejection_body(InboundRejection::invalid_request(
                "request_produced_no_response",
                Some(id),
                "request produced no response",
            )),
        );
    }
    // A plain HTTP caller has no other channel, so its response carries everything queued for
    // the connection, in sequence order, ending with this reply.
    let messages = exchange.transfer(&connection_id, |messages| {
        messages
            .into_iter()
            .map(WireMessage::from)
            .collect::<Vec<_>>()
    });
    json_response(
        200,
        serde_json::to_value(messages).expect("wire messages serialize"),
    )
}

/// Reads one JSON-RPC message body.
pub(super) fn parse_protocol_message(
    body: &str,
) -> Result<InboundProtocolMessage, InboundRejection> {
    let value = serde_json::from_str::<Value>(body).map_err(parse_rejection)?;
    parse_inbound(value)
}

/// An HTTP 400 carrying the JSON-RPC error and a stable, non-sensitive reason code.
fn protocol_rejection(rejection: InboundRejection) -> LocalHttpResponse {
    crate::logging::warn(
        "local_http_protocol_rejected",
        json!({ "reason_code": rejection.reason_code }),
    );
    json_response(400, rejection_body(rejection))
}

pub(super) fn rejection_body(rejection: InboundRejection) -> Value {
    let reason_code = rejection.reason_code;
    let mut value = serde_json::to_value(rejection.into_wire()).expect("wire message serializes");
    if let Some(object) = value.as_object_mut() {
        object.insert("code".to_string(), json!(reason_code));
    }
    value
}

pub(super) fn valid_connection_id(value: Option<&str>) -> Option<ConnectionId> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return None;
    }
    Some(ConnectionId::new(format!("local-http:{value}")))
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
