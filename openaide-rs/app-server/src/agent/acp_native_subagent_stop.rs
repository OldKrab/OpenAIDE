//! Stops one Native Subagent on the shared Agent connection.
//!
//! ACP stops a Subagent with `session/cancel` addressed to the child session.
//! The notification has no response, so success only proves the connection
//! took it; the Subagent's status changes through the Agent's own later
//! `subagent_update`, never through this call.

use std::time::Instant;

use agent_client_protocol::{Agent, ConnectionTo};
use serde_json::json;

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_schema::{CancelNotification, SessionId};
use crate::agent::acp_trace::AcpTraceSession;
use crate::logging;
use crate::protocol::errors::RuntimeError;

const OPERATION: &str = "session/cancel";

pub(super) fn stop_native_subagent(
    connection: &ConnectionTo<Agent>,
    parent_session_id: &SessionId,
    trace: Option<&AcpTraceSession>,
    native_session_id: String,
) -> Result<(), RuntimeError> {
    let started = Instant::now();
    logging::info(
        "acp_subagent_stop_started",
        json!({
            "operation": OPERATION,
            "session_id": parent_session_id.to_string(),
            "child_session_id": native_session_id,
            "attempt": 1,
        }),
    );
    let notification = CancelNotification::new(SessionId::new(native_session_id.clone()));
    if let Some(trace) = trace {
        trace.record(
            "client_to_agent",
            "session/cancel.subagent_notification",
            &notification,
        );
    }
    let result = connection
        .send_notification(notification)
        .map_err(acp_error);
    logging::info(
        "acp_subagent_stop",
        json!({
            "operation": OPERATION,
            "session_id": parent_session_id.to_string(),
            "child_session_id": native_session_id,
            "attempt": 1,
            "outcome": if result.is_ok() { "sent" } else { "failed" },
            "error_code": result.as_ref().err().map(|error| error.code()),
            "error_kind": result.as_ref().err().map(|error| error.reason()),
            "duration_ms": started.elapsed().as_millis(),
        }),
    );
    result
}
