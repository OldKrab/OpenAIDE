use crate::agent::acp_schema::{CloseSessionRequest, DeleteSessionRequest, SessionId};
use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_trace::AcpTraceSession;
use crate::protocol::errors::RuntimeError;

#[cfg(test)]
#[path = "acp_session_termination_tests.rs"]
mod tests;

pub(super) async fn close_active_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    supports_session_close: bool,
    trace: Option<&AcpTraceSession>,
) {
    if !supports_session_close {
        return;
    }
    let request = CloseSessionRequest::new(session_id);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/close.request", &request);
    }
    match connection.send_request(request).block_task().await {
        Ok(response) => {
            if let Some(trace) = trace {
                trace.record("agent_to_client", "session/close.response", &response);
            }
        }
        Err(error) => {
            if let Some(trace) = trace {
                trace.record_value(
                    "agent_to_client",
                    "session/close.error",
                    serde_json::json!({ "error": error.to_string() }),
                );
            }
        }
    }
}

pub(super) async fn delete_active_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    supports_session_delete: bool,
    trace: Option<&AcpTraceSession>,
) -> Result<(), RuntimeError> {
    if !supports_session_delete {
        return Err(RuntimeError::CapabilityMissing(
            "agent session delete is not available".to_string(),
        ));
    }
    let request = DeleteSessionRequest::new(session_id);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/delete.request", &request);
    }
    match connection.send_request(request).block_task().await {
        Ok(response) => {
            if let Some(trace) = trace {
                trace.record("agent_to_client", "session/delete.response", &response);
            }
            Ok(())
        }
        Err(error) => {
            if let Some(trace) = trace {
                trace.record_value(
                    "agent_to_client",
                    "session/delete.error",
                    serde_json::json!({ "error": error.to_string() }),
                );
            }
            Err(acp_error(error))
        }
    }
}
