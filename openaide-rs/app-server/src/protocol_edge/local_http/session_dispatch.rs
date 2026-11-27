//! HTTP mapping of the reliable session: authenticates each request, then
//! turns registry outcomes into status codes. Frame dispatch and delivery
//! queueing are shared with pushing links.

use openaide_app_server_protocol::methods::AGENT_AUTHENTICATE;
use serde_json::{json, Value};

use crate::client_lifecycle::ConnectionId;
use crate::protocol_edge::concurrent_reads::{is_concurrent_read, request_method};
use crate::protocol_edge::outbox::ProtocolExchange;
use crate::protocol_edge::reliable_session::{
    AcceptClientFrame, PollError, ReliableSessionRegistry,
};
use crate::protocol_edge::stdio::wire::{parse_inbound, WireMessage};

use super::protocol::{rejection_body, valid_connection_id};
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
    exchange: &impl ProtocolExchange,
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
        exchange,
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
    exchange: &impl ProtocolExchange,
) -> LocalHttpResponse {
    match auth_status(authorization, expected_token) {
        AuthStatus::Authorized => {}
        AuthStatus::Missing => return empty_response(401),
        AuthStatus::Invalid => return empty_response(403),
    }
    let Some(connection_id) = valid_connection_id(connection_id) else {
        return reliable_upload_rejection("invalid_connection_id", "single");
    };
    dispatch_accepted_session_frame(connection_id, accepted, sessions, exchange).0
}

/// Runs an accepted frame, then moves everything queued for its connection, the reply
/// included, onto the frame's session. RPC replies travel on that ordered sequence, never
/// inline. Returns the response and how many frames joined the session.
pub(super) fn dispatch_accepted_session_frame(
    connection_id: ConnectionId,
    accepted: AcceptedReliableUpload,
    sessions: &ReliableSessionRegistry,
    exchange: &impl ProtocolExchange,
) -> (LocalHttpResponse, usize) {
    let message = match parse_inbound(accepted.message) {
        Ok(message) => message,
        Err(rejection) => {
            log_reliable_upload_rejection(rejection.reason_code, "single");
            return (json_response(400, rejection_body(rejection)), 0);
        }
    };
    exchange.dispatch(connection_id.clone(), message);
    let queued = transfer_to_session(exchange, sessions, &accepted.session_id, &connection_id);
    (empty_response(204), queued)
}

/// Moves a connection's outbound sequence onto one of its sessions, in order.
pub(super) fn transfer_to_session(
    exchange: &impl ProtocolExchange,
    sessions: &ReliableSessionRegistry,
    session_id: &str,
    connection_id: &ConnectionId,
) -> usize {
    exchange.transfer(connection_id, |messages| {
        let mut queued = 0;
        let mut dropped = 0;
        for message in messages {
            let message =
                serde_json::to_value(WireMessage::from(message)).expect("wire message serializes");
            if sessions.enqueue_server_message(session_id, message) {
                queued += 1;
            } else {
                dropped += 1;
            }
        }
        if dropped > 0 {
            crate::logging::warn(
                "reliable_session_transfer_dropped",
                json!({
                    "connection_id": connection_id.as_str(),
                    "session_id": session_id,
                    "dropped_count": dropped,
                    "reason_code": "unknown_session",
                }),
            );
        }
        queued
    })
}

pub(super) fn is_agent_authenticate_request(message: &Value) -> bool {
    request_method(message).as_deref() == Some(AGENT_AUTHENTICATE)
}

/// Whether the frame is a request a link may run beside its ordered dispatch; see
/// [`crate::protocol_edge::concurrent_reads`] for the ordering argument.
pub(super) fn is_concurrent_read_request(message: &Value) -> bool {
    request_method(message).is_some_and(|method| is_concurrent_read(&method))
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

pub(super) fn handle_reliable_session_poll(
    authorization: Option<&str>,
    expected_token: &str,
    connection_id: Option<&str>,
    session_id: &str,
    after: u64,
    sessions: &ReliableSessionRegistry,
    exchange: &impl ProtocolExchange,
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
    if !exchange.observe_link(&connection_id) {
        return empty_response(410);
    }
    transfer_to_session(exchange, sessions, session_id, &connection_id);
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
