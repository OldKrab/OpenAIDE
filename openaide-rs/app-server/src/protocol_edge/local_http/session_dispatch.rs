//! HTTP mapping of the reliable session: authenticates each request, then
//! turns registry outcomes into status codes. Frame dispatch and delivery
//! queueing are shared with pushing links.

use openaide_app_server_protocol::methods::AGENT_AUTHENTICATE;
use serde_json::{json, Value};

use crate::client_lifecycle::ConnectionId;
use crate::protocol_edge::reliable_session::{
    AcceptClientFrame, PollError, ReliableSessionRegistry,
};
use crate::protocol_edge::stdio::wire::{
    event_wire_messages, server_request_wire_messages, WireRequest, WireRequestId,
};
use crate::protocol_edge::{GatewayOutcome, InboundProtocolMessage};

use super::protocol::{dispatch_protocol_message, handle_local_http_protocol, valid_connection_id};
use super::reliable_upload_chunks::AppendError as ReliableChunkError;
use super::{auth_status, empty_response, json_response, AuthStatus, LocalHttpResponse};

pub(super) fn reliable_chunk_error_response(error: ReliableChunkError) -> LocalHttpResponse {
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

pub(super) fn handle_reliable_session_open(
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
pub(super) fn handle_reliable_session_upload(
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

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReliableUpload {
    session_id: String,
    sequence: u64,
    message: Value,
}

pub(crate) struct AcceptedReliableUpload {
    pub(super) session_id: String,
    pub(super) message: Value,
}

pub(super) fn accept_reliable_session_upload(
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

pub(super) fn dispatch_reliable_session_upload(
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
    queue_dispatch_replies(response, &accepted.session_id, sessions)
}

/// Runs an accepted frame for a link that authenticated when it attached.
pub(super) fn dispatch_accepted_session_frame(
    connection_id: ConnectionId,
    accepted: AcceptedReliableUpload,
    sessions: &ReliableSessionRegistry,
    dispatch: impl FnOnce(ConnectionId, InboundProtocolMessage) -> GatewayOutcome,
) -> LocalHttpResponse {
    let response = dispatch_protocol_message(
        connection_id,
        &accepted.message.to_string(),
        dispatch,
        |_| Vec::new(),
    );
    queue_dispatch_replies(response, &accepted.session_id, sessions)
}

/// RPC replies travel on the session's ordered server sequence, never inline.
fn queue_dispatch_replies(
    response: LocalHttpResponse,
    session_id: &str,
    sessions: &ReliableSessionRegistry,
) -> LocalHttpResponse {
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
            sessions.enqueue_server_message(session_id, message);
        }
    }
    empty_response(204)
}

pub(super) fn is_agent_authenticate_request(message: &Value) -> bool {
    let Ok(request) = serde_json::from_value::<WireRequest>(message.clone()) else {
        return false;
    };
    request.jsonrpc == "2.0"
        && matches!(request.id, WireRequestId::Request(_))
        && request.method.as_deref() == Some(AGENT_AUTHENTICATE)
}

pub(super) fn reliable_upload_rejection(
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

pub(super) fn handle_reliable_session_poll(
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
    let Some((events, server_requests)) = receive(&connection_id) else {
        return empty_response(410);
    };
    queue_connection_deliveries(sessions, session_id, connection_id, events, server_requests);
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

/// Moves a connection's drained gateway deliveries onto its session sequence.
pub(super) fn queue_connection_deliveries(
    sessions: &ReliableSessionRegistry,
    session_id: &str,
    connection_id: ConnectionId,
    events: Vec<crate::protocol_edge::GatewayEventDelivery>,
    mut server_requests: Vec<crate::server_requests::ServerRequestDelivery>,
) {
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
}
