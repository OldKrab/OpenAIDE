use serde_json::{json, Value};

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::protocol_edge::stdio::wire::{
    client_response, event_wire_messages, id_to_gateway_id,
    invalid_request as wire_invalid_request, parse_error, server_request_wire_messages,
    wire_messages, WireMessage, WireRequest, WireRequestId,
};
use crate::protocol_edge::{
    GatewayOutcome, GatewayResponse, InboundProtocolMessage, SharedRpcGateway,
};

use super::event_streams::{EventStreamLease, EventStreamRegistry};
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
struct ReliableClose {
    session_id: String,
}

pub struct LocalHttpProtocolHandler {
    gateway: SharedRpcGateway,
    auth_token: String,
    event_streams: EventStreamRegistry,
    sessions: ReliableSessionRegistry,
}

impl Clone for LocalHttpProtocolHandler {
    fn clone(&self) -> Self {
        Self {
            gateway: self.gateway.clone(),
            auth_token: self.auth_token.clone(),
            event_streams: self.event_streams.clone(),
            sessions: self.sessions.clone(),
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
        }
    }

    pub fn handle(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        body: &str,
    ) -> LocalHttpResponse {
        let reliable_transport = serde_json::from_str::<Value>(body).ok().and_then(|value| {
            value
                .get("transport")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        if reliable_transport.as_deref() == Some("open") {
            return handle_reliable_session_open(
                authorization,
                &self.auth_token,
                connection_id,
                &self.sessions,
            );
        }
        if reliable_transport.as_deref() == Some("close") {
            return handle_reliable_session_close(
                authorization,
                &self.auth_token,
                connection_id,
                body,
                &self.sessions,
            );
        }
        if reliable_transport.as_deref() == Some("send") {
            let now = AppServerTime::now();
            let gateway = self.gateway.clone();
            return handle_reliable_session_upload(
                authorization,
                &self.auth_token,
                connection_id,
                body,
                &self.sessions,
                |connection_id, message| gateway.handle_inbound(connection_id, message, now),
            );
        }
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
    }

    pub fn poll_session(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        session_id: &str,
        after: u64,
    ) -> LocalHttpResponse {
        let connection = valid_connection_id(connection_id);
        handle_reliable_session_poll(
            authorization,
            &self.auth_token,
            connection_id,
            session_id,
            after,
            &self.sessions,
            || {
                let Some(connection_id) = connection else {
                    return (Vec::new(), Vec::new());
                };
                (
                    self.gateway
                        .drain_event_deliveries_for_connection(&connection_id),
                    self.gateway
                        .drain_server_requests_for_connection(&connection_id, AppServerTime::now()),
                )
            },
        )
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
                .observe_event_stream_activity(lease.connection_id(), AppServerTime::now())
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
        return json_response(
            400,
            wire_value(wire_invalid_request(
                None,
                "missing or invalid X-OpenAIDE-Connection-Id".to_string(),
            )),
        );
    };
    let value = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => return json_response(400, wire_value(parse_error(error))),
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
            return json_response(
                400,
                wire_value(wire_invalid_request(None, error.to_string())),
            )
        }
    };
    let id = match request.id {
        WireRequestId::Notification => {
            return json_response(
                400,
                wire_value(wire_invalid_request(
                    None,
                    "notifications are not supported".into(),
                )),
            );
        }
        WireRequestId::Invalid => {
            return json_response(
                400,
                wire_value(wire_invalid_request(
                    Some(Value::Null),
                    "invalid JSON-RPC id".into(),
                )),
            );
        }
        WireRequestId::Request(id) => id,
    };
    if request.jsonrpc != "2.0" {
        return json_response(
            400,
            wire_value(wire_invalid_request(Some(id), "jsonrpc must be 2.0".into())),
        );
    }
    let Some(method) = request.method else {
        return json_response(
            400,
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

fn handle_reliable_session_close(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    body: &str,
    sessions: &ReliableSessionRegistry,
) -> LocalHttpResponse {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return empty_response(401),
        AuthStatus::Invalid => return empty_response(403),
    }
    let Some(connection_id) = valid_connection_id(connection_id) else {
        return empty_response(400);
    };
    let close = match serde_json::from_str::<ReliableClose>(body) {
        Ok(close) => close,
        Err(_) => return empty_response(400),
    };
    if !sessions.close(&close.session_id, &connection_id) {
        return empty_response(410);
    }
    json_response(200, json!({ "sessionId": close.session_id }))
}

fn handle_reliable_session_upload(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    body: &str,
    sessions: &ReliableSessionRegistry,
    dispatch: impl FnOnce(ConnectionId, InboundProtocolMessage) -> GatewayOutcome,
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
    let upload = match serde_json::from_str::<ReliableUpload>(body) {
        Ok(upload) => upload,
        Err(_) => return empty_response(400),
    };
    let session_id = upload.session_id.clone();
    let mut dispatched = None;
    let accepted = sessions.accept_client_frame(
        &session_id,
        &connection_id,
        upload.sequence,
        upload.message,
        |message| {
            dispatched = Some(handle_local_http_protocol(
                authorization,
                expected_token,
                raw_connection_id,
                &message.to_string(),
                dispatch,
                |_| Vec::new(),
            ));
        },
    );
    match accepted {
        AcceptClientFrame::Duplicate => return empty_response(204),
        AcceptClientFrame::Gap { expected } => {
            return json_response(409, json!({ "expectedSequence": expected }))
        }
        AcceptClientFrame::UnknownSession => return empty_response(410),
        AcceptClientFrame::WrongConnection => return empty_response(403),
        AcceptClientFrame::Accepted => {}
    }
    let Some(response) = dispatched else {
        return empty_response(500);
    };
    if response.status != 200 {
        return response;
    }
    if let Ok(value) = serde_json::from_str::<Value>(&response.body) {
        for message in value.as_array().cloned().unwrap_or_else(|| vec![value]) {
            sessions.enqueue_server_message(&session_id, message);
        }
    }
    empty_response(204)
}

fn handle_reliable_session_poll(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    session_id: &str,
    after: u64,
    sessions: &ReliableSessionRegistry,
    drain: impl FnOnce() -> (
        Vec<crate::protocol_edge::GatewayEventDelivery>,
        Vec<crate::server_requests::ServerRequestDelivery>,
    ),
) -> LocalHttpResponse {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return empty_response(401),
        AuthStatus::Invalid => return empty_response(403),
    }
    let Some(connection_id) = valid_connection_id(connection_id) else {
        return empty_response(400);
    };
    if sessions.connection_id(session_id).as_ref() != Some(&connection_id) {
        return empty_response(410);
    }
    let (events, mut server_requests) = drain();
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

fn valid_connection_id(value: Option<&str>) -> Option<ConnectionId> {
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
