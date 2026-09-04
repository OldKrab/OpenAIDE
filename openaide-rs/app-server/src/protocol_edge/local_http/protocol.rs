use openaide_app_server_protocol::methods::{AGENT_AUTHENTICATE, CLIENT_HEARTBEAT};
use serde_json::{json, Value};

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::logging;
use crate::protocol_edge::stdio::wire::{
    client_response, event_wire_messages, id_to_gateway_id,
    invalid_request as wire_invalid_request, parse_error, server_request_wire_messages,
    wire_messages, WireMessage, WireRequest, WireRequestId,
};
use crate::protocol_edge::{
    GatewayOutcome, GatewayResponse, InboundProtocolMessage, SharedRpcGateway,
};

use super::event_streams::{EventStreamLease, EventStreamRegistry};
use super::reliable_upload_chunks::{
    AppendError as ReliableChunkError, AppendOutcome as ReliableChunkOutcome,
    ReliableUploadChunkRegistry,
};
use super::sessions::{AcceptClientFrame, PollError, ReliableSessionRegistry};
use super::{auth_status, empty_response, json_response, AuthStatus, LocalHttpResponse};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReliableUpload {
    session_id: String,
    sequence: u64,
    message: Value,
}

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
    gateway: SharedRpcGateway,
    auth_token: String,
    event_streams: EventStreamRegistry,
    sessions: ReliableSessionRegistry,
    upload_chunks: ReliableUploadChunkRegistry,
}

impl Clone for LocalHttpProtocolHandler {
    fn clone(&self) -> Self {
        Self {
            gateway: self.gateway.clone(),
            auth_token: self.auth_token.clone(),
            event_streams: self.event_streams.clone(),
            sessions: self.sessions.clone(),
            upload_chunks: self.upload_chunks.clone(),
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
            gateway,
            auth_token: auth_token.into(),
            event_streams: EventStreamRegistry::default(),
            sessions: ReliableSessionRegistry::new(server_id),
            upload_chunks: ReliableUploadChunkRegistry::default(),
        }
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
        let started_at = std::time::Instant::now();
        if should_log {
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
            let now = AppServerTime::now();
            let gateway = self.gateway.clone();
            handle_local_http_protocol(
                authorization,
                &self.auth_token,
                connection_id,
                body,
                |connection_id, message| gateway.handle_inbound(connection_id, message, now),
                |connection_id| {
                    if self.event_streams.is_active(connection_id) {
                        Vec::new()
                    } else {
                        self.gateway
                            .drain_event_deliveries_for_connection(connection_id)
                    }
                },
            )
        };
        if should_log {
            logging::info(
                "local_http_request_completed",
                serde_json::json!({
                    "connection_id": connection_id,
                    "transport": transport_kind,
                    "method": method,
                    "http_status": response.status,
                    "duration_ms": started_at.elapsed().as_millis(),
                }),
            );
        }
        response
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
            let now = AppServerTime::now();
            let gateway = self.gateway.clone();
            return dispatch_reliable_session_upload(
                authorization,
                &self.auth_token,
                connection_id,
                accepted,
                &self.sessions,
                move |connection_id, message| gateway.handle_inbound(connection_id, message, now),
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
            let now = AppServerTime::now();
            let response = dispatch_reliable_session_upload(
                authorization.as_deref(),
                &auth_token,
                raw_connection_id.as_deref(),
                accepted,
                &sessions,
                move |connection_id, message| gateway.handle_inbound(connection_id, message, now),
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
        let now = AppServerTime::now();
        let started_at = std::time::Instant::now();
        let response = handle_reliable_session_poll(
            authorization,
            &self.auth_token,
            connection_id,
            session_id,
            after,
            &self.sessions,
            |connection_id| {
                self.gateway
                    .observe_connection_activity(connection_id, now)
                    .then(|| {
                        (
                            self.gateway
                                .drain_event_deliveries_for_connection(connection_id),
                            self.gateway
                                .drain_server_requests_for_connection(connection_id, now),
                        )
                    })
            },
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

    pub(crate) fn begin_event_stream(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
    ) -> Result<EventStreamLease, LocalHttpResponse> {
        match auth_status(authorization, &self.auth_token) {
            AuthStatus::Authorized => {}
            AuthStatus::Missing => return Err(empty_response(401)),
            AuthStatus::Invalid => return Err(empty_response(403)),
        }
        let connection_id =
            valid_connection_id(connection_id).ok_or_else(|| empty_response(400))?;
        if !self.gateway.connection_is_initialized(&connection_id) {
            return Err(empty_response(409));
        }
        Ok(self.event_streams.begin(connection_id))
    }

    pub(crate) fn event_stream_is_current(&self, lease: &EventStreamLease) -> bool {
        self.event_streams.is_current(lease)
    }

    pub(crate) fn observe_event_stream_activity(&self, lease: &EventStreamLease) -> bool {
        self.event_streams.is_current(lease)
            && self
                .gateway
                .observe_connection_activity(lease.connection_id(), AppServerTime::now())
    }

    pub(crate) fn finish_event_stream(&self, lease: &EventStreamLease) {
        self.event_streams.finish(lease);
    }

    pub(crate) fn drain_push_messages(&self, lease: &EventStreamLease) -> String {
        self.event_streams
            .with_current(lease, || {
                let connection_id = lease.connection_id();
                let events = self
                    .gateway
                    .drain_event_deliveries_for_connection(connection_id);
                let server_requests = self
                    .gateway
                    .drain_server_requests_for_connection(connection_id, AppServerTime::now());
                if events.is_empty() && server_requests.is_empty() {
                    return String::new();
                }
                serde_json::to_string(
                    &event_wire_messages(connection_id.clone(), events)
                        .into_iter()
                        .chain(server_request_wire_messages(
                            connection_id.clone(),
                            server_requests,
                        ))
                        .collect::<Vec<_>>(),
                )
                .expect("LocalHttp push messages serialize")
            })
            .unwrap_or_default()
    }
}

fn handle_local_http_protocol(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    body: &str,
    dispatch: impl FnOnce(ConnectionId, InboundProtocolMessage) -> GatewayOutcome,
    drain_events: impl FnOnce(&ConnectionId) -> Vec<crate::protocol_edge::GatewayEventDelivery>,
) -> LocalHttpResponse {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return empty_response(401),
        AuthStatus::Invalid => return empty_response(403),
    }
    let raw_connection_id = connection_id;
    let Some(connection_id) = valid_connection_id(raw_connection_id) else {
        return protocol_rejection(
            "invalid_connection_id",
            wire_value(wire_invalid_request(
                None,
                "missing or invalid X-OpenAIDE-Connection-Id".to_string(),
            )),
        );
    };
    let value = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => return protocol_rejection("malformed_json", wire_value(parse_error(error))),
    };
    if let Some(response) = client_response(&value) {
        let InboundProtocolMessage::ClientResponse { request_id, .. } = &response else {
            unreachable!("client_response only returns client responses");
        };
        let request_id = request_id.clone();
        return match dispatch(connection_id.clone(), response) {
            GatewayOutcome::Respond {
                connection_id,
                response,
                events,
                server_requests,
                ..
            } => {
                if matches!(response, GatewayResponse::Error(_)) {
                    return json_response(
                        200,
                        serde_json::to_value(wire_messages(
                            Value::String(request_id),
                            connection_id,
                            response,
                            events,
                            server_requests,
                        ))
                        .expect("wire messages serialize"),
                    );
                }
                json_response(
                    200,
                    side_effect_messages(&connection_id, Vec::new(), server_requests, events),
                )
            }
            GatewayOutcome::Noop => json_response(
                200,
                side_effect_messages(&connection_id, Vec::new(), Vec::new(), Vec::new()),
            ),
        };
    }
    let request = match serde_json::from_value::<WireRequest>(value) {
        Ok(request) => request,
        Err(error) => {
            return protocol_rejection(
                "invalid_request_envelope",
                wire_value(wire_invalid_request(None, error.to_string())),
            )
        }
    };
    let id = match request.id {
        WireRequestId::Notification => {
            return protocol_rejection(
                "unsupported_notification",
                wire_value(wire_invalid_request(
                    None,
                    "notifications are not supported".into(),
                )),
            );
        }
        WireRequestId::Invalid => {
            return protocol_rejection(
                "invalid_request_id",
                wire_value(wire_invalid_request(
                    Some(Value::Null),
                    "invalid JSON-RPC id".into(),
                )),
            );
        }
        WireRequestId::Request(id) => id,
    };
    if request.jsonrpc != "2.0" {
        return protocol_rejection(
            "invalid_jsonrpc_version",
            wire_value(wire_invalid_request(Some(id), "jsonrpc must be 2.0".into())),
        );
    }
    let Some(method) = request.method else {
        return protocol_rejection(
            "missing_method",
            wire_value(wire_invalid_request(Some(id), "method is required".into())),
        );
    };
    let inbound = InboundProtocolMessage::ClientRequest {
        id: id_to_gateway_id(&id),
        method,
        params: request.params.unwrap_or_else(|| json!({})),
        meta: request.meta,
    };
    let queued_events = drain_events(&connection_id);
    match dispatch(connection_id.clone(), inbound) {
        GatewayOutcome::Respond {
            response,
            events,
            server_requests,
            ..
        } => json_response(
            200,
            serde_json::to_value(wire_messages(
                id,
                connection_id,
                response,
                queued_events.into_iter().chain(events).collect(),
                server_requests,
            ))
            .expect("wire messages serialize"),
        ),
        GatewayOutcome::Noop => json_response(
            500,
            wire_value(wire_invalid_request(
                Some(id),
                "request produced no response".into(),
            )),
        ),
    }
}

fn reliable_chunk_error_response(error: ReliableChunkError) -> LocalHttpResponse {
    let rejection_code = match error {
        ReliableChunkError::InvalidChunk => Some("invalid_chunk"),
        ReliableChunkError::InvalidUtf8 => Some("invalid_chunk_utf8"),
        _ => None,
    };
    if let Some(rejection_code) = rejection_code {
        return reliable_upload_rejection(rejection_code, "chunk");
    }
    let status = match error {
        ReliableChunkError::ChunkTooLarge | ReliableChunkError::UploadTooLarge => 413,
        ReliableChunkError::MetadataMismatch | ReliableChunkError::OffsetMismatch => 409,
        ReliableChunkError::StateUnavailable => 500,
        ReliableChunkError::InvalidChunk | ReliableChunkError::InvalidUtf8 => unreachable!(),
    };
    empty_response(status)
}

fn handle_reliable_session_open(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    sessions: &ReliableSessionRegistry,
) -> LocalHttpResponse {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return empty_response(401),
        AuthStatus::Invalid => return empty_response(403),
    }
    let raw_connection_id = connection_id;
    let Some(connection_id) = valid_connection_id(raw_connection_id) else {
        return empty_response(400);
    };
    let opened = sessions.open(connection_id);
    json_response(
        200,
        json!({
            "transportVersion": 1,
            "sessionId": opened.session_id,
            "serverId": opened.server_id,
        }),
    )
}

#[cfg(test)]
fn handle_reliable_session_upload(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    body: &str,
    sessions: &ReliableSessionRegistry,
    dispatch: impl FnOnce(ConnectionId, InboundProtocolMessage) -> GatewayOutcome,
) -> LocalHttpResponse {
    let accepted = match accept_reliable_session_upload(
        authorization,
        expected_token,
        connection_id,
        body,
        sessions,
    ) {
        Ok(accepted) => accepted,
        Err(response) => return response,
    };
    dispatch_reliable_session_upload(
        authorization,
        expected_token,
        connection_id,
        accepted,
        sessions,
        dispatch,
    )
}

struct AcceptedReliableUpload {
    session_id: String,
    message: Value,
}

fn accept_reliable_session_upload(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    body: &str,
    sessions: &ReliableSessionRegistry,
) -> Result<AcceptedReliableUpload, LocalHttpResponse> {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return Err(empty_response(401)),
        AuthStatus::Invalid => return Err(empty_response(403)),
    }
    let raw_connection_id = connection_id;
    let Some(connection_id) = valid_connection_id(raw_connection_id) else {
        return Err(reliable_upload_rejection("invalid_connection_id", "single"));
    };
    let upload = match serde_json::from_str::<ReliableUpload>(body) {
        Ok(upload) => upload,
        Err(_) => {
            return Err(reliable_upload_rejection(
                "invalid_upload_envelope",
                "single",
            ))
        }
    };
    let session_id = upload.session_id.clone();
    let mut accepted_message = None;
    let accepted = sessions.accept_client_frame(
        &session_id,
        &connection_id,
        upload.sequence,
        upload.message,
        |message| {
            accepted_message = Some(message);
        },
    );
    match accepted {
        AcceptClientFrame::Duplicate => return Err(empty_response(204)),
        AcceptClientFrame::Gap { expected } => {
            return Err(json_response(409, json!({ "expectedSequence": expected })))
        }
        AcceptClientFrame::UnknownSession => return Err(empty_response(410)),
        AcceptClientFrame::WrongConnection => return Err(empty_response(403)),
        AcceptClientFrame::Accepted => {}
    }
    let Some(message) = accepted_message else {
        return Err(empty_response(500));
    };
    Ok(AcceptedReliableUpload {
        session_id,
        message,
    })
}

fn dispatch_reliable_session_upload(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    accepted: AcceptedReliableUpload,
    sessions: &ReliableSessionRegistry,
    dispatch: impl FnOnce(ConnectionId, InboundProtocolMessage) -> GatewayOutcome,
) -> LocalHttpResponse {
    let response = handle_local_http_protocol(
        authorization,
        expected_token,
        connection_id,
        &accepted.message.to_string(),
        dispatch,
        |_| Vec::new(),
    );
    if response.status != 200 {
        if response.status == 400 {
            let reason_code =
                response_code(&response).unwrap_or_else(|| "nested_protocol_rejected".to_string());
            log_reliable_upload_rejection(&reason_code, "single");
        }
        return response;
    }
    if let Ok(value) = serde_json::from_str::<Value>(&response.body) {
        for message in value.as_array().cloned().unwrap_or_else(|| vec![value]) {
            sessions.enqueue_server_message(&accepted.session_id, message);
        }
    }
    empty_response(204)
}

fn is_agent_authenticate_request(message: &Value) -> bool {
    let Ok(request) = serde_json::from_value::<WireRequest>(message.clone()) else {
        return false;
    };
    request.jsonrpc == "2.0"
        && matches!(request.id, WireRequestId::Request(_))
        && request.method.as_deref() == Some(AGENT_AUTHENTICATE)
}

/// Adds a stable, non-sensitive reason to protocol-level HTTP 400 responses.
fn protocol_rejection(reason_code: &'static str, mut value: Value) -> LocalHttpResponse {
    crate::logging::warn(
        "local_http_protocol_rejected",
        json!({ "reason_code": reason_code }),
    );
    if let Some(object) = value.as_object_mut() {
        object.insert("code".to_string(), json!(reason_code));
    }
    json_response(400, value)
}

fn reliable_upload_rejection(
    reason_code: &'static str,
    upload_kind: &'static str,
) -> LocalHttpResponse {
    log_reliable_upload_rejection(reason_code, upload_kind);
    json_response(400, json!({ "code": reason_code }))
}

fn log_reliable_upload_rejection(reason_code: &str, upload_kind: &'static str) {
    crate::logging::warn(
        "reliable_session_upload_rejected",
        json!({
            "reason_code": reason_code,
            "upload_kind": upload_kind,
        }),
    );
}

fn response_code(response: &LocalHttpResponse) -> Option<String> {
    serde_json::from_str::<Value>(&response.body)
        .ok()?
        .get("code")?
        .as_str()
        .map(str::to_string)
}

fn handle_reliable_session_poll(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    session_id: &str,
    after: u64,
    sessions: &ReliableSessionRegistry,
    receive: impl FnOnce(
        &ConnectionId,
    ) -> Option<(
        Vec<crate::protocol_edge::GatewayEventDelivery>,
        Vec<crate::server_requests::ServerRequestDelivery>,
    )>,
) -> LocalHttpResponse {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return empty_response(401),
        AuthStatus::Invalid => return empty_response(403),
    }
    let Some(connection_id) = valid_connection_id(connection_id) else {
        crate::logging::warn(
            "reliable_session_poll_rejected",
            json!({ "reason_code": "invalid_connection_id" }),
        );
        return json_response(400, json!({ "code": "invalid_connection_id" }));
    };
    if sessions.connection_id(session_id).as_ref() != Some(&connection_id) {
        return empty_response(410);
    }
    let Some((events, mut server_requests)) = receive(&connection_id) else {
        return empty_response(410);
    };
    // Task-scoped permissions and questions are shared product state. Their
    // snapshots/events fan out to every eligible client; only client-targeted
    // capabilities remain reverse RPC requests.
    server_requests.retain(|request| {
        !matches!(
            request.envelope.method.as_str(),
            openaide_app_server_protocol::server_requests::PERMISSION_REQUEST
                | openaide_app_server_protocol::server_requests::QUESTION_REQUEST
        )
    });
    for message in event_wire_messages(connection_id.clone(), events)
        .into_iter()
        .chain(server_request_wire_messages(connection_id, server_requests))
    {
        sessions.enqueue_server_message(
            session_id,
            serde_json::to_value(message).expect("wire message serializes"),
        );
    }
    match sessions.poll(session_id, after) {
        Ok(batch) if batch.frames.is_empty() => empty_response(204),
        Ok(batch) => json_response(
            200,
            serde_json::to_value(batch).expect("session batch serializes"),
        ),
        Err(PollError::UnknownSession) => empty_response(410),
        Err(PollError::InvalidAcknowledgement) => empty_response(409),
        Err(PollError::ReplayExpired) => json_response(409, json!({ "resyncRequired": true })),
    }
}

fn side_effect_messages(
    connection_id: &ConnectionId,
    first_events: Vec<crate::protocol_edge::GatewayEventDelivery>,
    server_requests: Vec<crate::server_requests::ServerRequestDelivery>,
    later_events: Vec<crate::protocol_edge::GatewayEventDelivery>,
) -> Value {
    serde_json::to_value(
        event_wire_messages(
            connection_id.clone(),
            first_events.into_iter().chain(later_events).collect(),
        )
        .into_iter()
        .chain(server_request_wire_messages(
            connection_id.clone(),
            server_requests,
        ))
        .collect::<Vec<_>>(),
    )
    .expect("wire messages serialize")
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

fn wire_value(message: WireMessage) -> Value {
    serde_json::to_value(message).expect("wire message serializes")
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
