use std::time::Instant;

use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_schema::{InitializeResponse, LogoutRequest};
use crate::logging;
use crate::protocol::errors::RuntimeError;

/// Ends the Agent-owned authentication state on the existing initialized connection.
pub(super) async fn logout_on_shared_process(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    agent_id: &str,
) -> Result<(), RuntimeError> {
    if initialize.agent_capabilities.auth.logout.is_none() {
        return Err(RuntimeError::CapabilityMissing(format!(
            "agent_logout:{agent_id}"
        )));
    }
    let started_at = Instant::now();
    logging::info(
        "acp_logout_started",
        serde_json::json!({ "agent_id": agent_id }),
    );
    let result = connection
        .send_request(LogoutRequest::new())
        .block_task()
        .await
        .map(|_| ())
        .map_err(acp_error);
    match &result {
        Ok(()) => logging::info(
            "acp_logout_completed",
            serde_json::json!({
                "agent_id": agent_id,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        ),
        Err(error) => logging::warn(
            "acp_logout_failed",
            serde_json::json!({
                "agent_id": agent_id,
                "duration_ms": started_at.elapsed().as_millis(),
                "error_kind": error.reason(),
            }),
        ),
    }
    result
}
