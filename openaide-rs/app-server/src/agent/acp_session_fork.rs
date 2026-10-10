//! Forks an Agent session on a shared Agent process.

use std::time::Instant;

use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_schema::{CloseSessionRequest, ForkSessionRequest, InitializeResponse};
use crate::agent::acp_session_capabilities::validate_session_fork_capabilities;
use crate::agent::{AgentForkedSession, AgentSessionFork};
use crate::logging;
use crate::protocol::errors::RuntimeError;

pub(super) async fn fork_session_on_shared_process(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    request: AgentSessionFork,
) -> Result<AgentForkedSession, RuntimeError> {
    validate_session_fork_capabilities(initialize)?;
    let mcp_servers = match request.secret_resolver {
        Some(resolver) => {
            resolver.resolve_mcp_servers(&initialize.agent_capabilities.mcp_capabilities)?
        }
        None => Vec::new(),
    };
    let started_at = Instant::now();
    logging::info(
        "acp_session_fork_started",
        serde_json::json!({
            "agent_id": request.agent_id,
            "source_session_id": request.source_session_id,
        }),
    );
    let response = connection
        .send_request(
            ForkSessionRequest::new(request.source_session_id.clone(), request.cwd)
                .mcp_servers(mcp_servers),
        )
        .block_task()
        .await
        .map_err(acp_error)?;
    let session_id = response.session_id.to_string();
    let close_warning = connection
        .send_request(CloseSessionRequest::new(response.session_id))
        .block_task()
        .await
        .is_err();
    logging::info(
        "acp_session_fork_completed",
        serde_json::json!({
            "agent_id": request.agent_id,
            "source_session_id": request.source_session_id,
            "forked_session_id": session_id,
            "duration_ms": started_at.elapsed().as_millis(),
            "close_warning": close_warning,
        }),
    );
    Ok(AgentForkedSession {
        session_id,
        close_warning,
    })
}
