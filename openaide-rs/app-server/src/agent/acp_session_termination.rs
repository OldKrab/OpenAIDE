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
    let request = CloseSessionRequest::new(session_id.clone());
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/close.request", &request);
    }
    match connection.send_request(request).block_task().await {
        Ok(response) => {
            if let Some(trace) = trace {
                trace.record("agent_to_client", "session/close.response", &response);
            }
            crate::logging::info(
                "acp_session_close_completed",
                serde_json::json!({ "session_id": session_id.to_string() }),
            );
        }
        Err(error) => {
            if let Some(trace) = trace {
                trace.record_value(
                    "agent_to_client",
                    "session/close.error",
                    serde_json::json!({ "error": error.to_string() }),
                );
            }
            crate::logging::warn(
                "acp_session_close_failed",
                serde_json::json!({
                    "session_id": session_id.to_string(),
                    "error": error.to_string(),
                }),
            );
        }
    }
}

pub(super) async fn delete_active_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    supports_session_delete: bool,
    trace: Option<&AcpTraceSession>,
    operation_id: &str,
) -> Result<(), RuntimeError> {
    if !supports_session_delete {
        return Err(RuntimeError::CapabilityMissing(
            "agent session delete is not available".to_string(),
        ));
    }
    let request = DeleteSessionRequest::new(session_id);
    let started_at = std::time::Instant::now();
    crate::logging::info(
        "acp_session_delete_started",
        serde_json::json!({
            "operation": "session/delete", "operation_id": operation_id, "session_id": request.session_id.to_string(), "attempt": 1,
        }),
    );
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/delete.request", &request);
    }
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        connection.send_request(request).block_task(),
    )
    .await;
    let result = match result {
        Ok(result) => result.map_err(|error| {
            if agent_client_protocol::is_incoming_transport_closed(&error) {
                RuntimeError::OutcomeUnknown(
                    "Session deletion outcome is unknown; retry explicitly".into(),
                )
            } else {
                acp_error(error)
            }
        }),
        Err(_) => Err(RuntimeError::OutcomeUnknown(
            "Session deletion outcome is unknown; retry explicitly".to_string(),
        )),
    };
    crate::logging::info(
        "acp_session_delete_completed",
        serde_json::json!({
            "operation": "session/delete", "operation_id": operation_id, "attempt": 1,
            "outcome": if result.is_ok() { "success" } else { "failure" },
            "duration_ms": started_at.elapsed().as_millis(),
            "error_kind": result.as_ref().err().map(RuntimeError::code),
        }),
    );
    match result {
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
                    serde_json::json!({ "error_kind": error.code() }),
                );
            }
            Err(error)
        }
    }
}

/// Deletion must leave the session event loop running: an earlier prompt/config
/// response can hold ACP's dispatch boundary until that loop consumes it.
#[derive(Default)]
pub(super) struct SessionDeleteRequest {
    pending: Option<(DeleteReply, DeleteFuture)>,
}

type DeleteReply = std::sync::mpsc::Sender<Result<(), RuntimeError>>;
type DeleteFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), RuntimeError>> + Send>>;

impl SessionDeleteRequest {
    pub(super) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn dispatch(
        &mut self,
        connection: &ConnectionTo<Agent>,
        session_id: SessionId,
        supported: bool,
        trace: Option<&AcpTraceSession>,
        reply: DeleteReply,
        operation_id: String,
    ) {
        if self.is_pending() {
            let _ = reply.send(Err(RuntimeError::NotReady(
                "Session deletion is already pending".into(),
            )));
            return;
        }
        let connection = connection.clone();
        let trace = trace.cloned();
        self.pending = Some((
            reply,
            Box::pin(async move {
                delete_active_session(
                    &connection,
                    session_id,
                    supported,
                    trace.as_ref(),
                    &operation_id,
                )
                .await
            }),
        ));
    }

    pub(super) async fn next_response(&mut self) -> (DeleteReply, Result<(), RuntimeError>) {
        let Some((_, future)) = self.pending.as_mut() else {
            return std::future::pending().await;
        };
        let result = future.await;
        let (reply, _) = self.pending.take().expect("pending deletion completed");
        (reply, result)
    }
}
