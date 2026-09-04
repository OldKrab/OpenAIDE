use std::collections::HashMap;
use std::time::Instant;

use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_schema::{
    AuthMethod, AuthMethodTerminal, AuthenticateRequest, InitializeResponse,
};
use crate::agent::acp_session_capabilities::validate_auth_method;
use crate::agent::AgentAuthenticateRequest;
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{AgentAuthenticateResult, AgentAuthenticateStatus};

/// Runs the complete shared-process authentication exchange without exposing its terminal or
/// diagnostics details to the process coordinator.
pub(super) async fn authenticate_on_shared_process(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    config: &AcpAgentConfig,
    host_bridge: &HostBridge,
    request: AgentAuthenticateRequest,
) -> Result<AgentAuthenticateResult, RuntimeError> {
    // Authentication on the shared process has no session trace, so this is the only record of
    // the ACP `authenticate` exchange. Method ids are agent-advertised identifiers, not secrets.
    let started_at = Instant::now();
    logging::info(
        "acp_authenticate_started",
        serde_json::json!({
            "agent_id": request.agent_id,
            "method_id": request.method_id,
            "terminal_confirmed": request.terminal_confirmed,
        }),
    );
    let result = authenticate_inner(connection, initialize, config, host_bridge, request).await;
    match &result {
        Ok(result) => logging::info(
            "acp_authenticate_completed",
            serde_json::json!({
                "agent_id": result.agent_id,
                "method_id": result.method_id,
                "status": result.status,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        ),
        Err(error) => logging::warn(
            "acp_authenticate_failed",
            serde_json::json!({
                "duration_ms": started_at.elapsed().as_millis(),
                "error_kind": error.reason(),
            }),
        ),
    }
    result
}

async fn authenticate_inner(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    config: &AcpAgentConfig,
    host_bridge: &HostBridge,
    request: AgentAuthenticateRequest,
) -> Result<AgentAuthenticateResult, RuntimeError> {
    validate_auth_method(initialize, &request.method_id)?;
    let method = initialize
        .auth_methods
        .iter()
        .find(|method| method.id().0.as_ref() == request.method_id)
        .ok_or_else(|| RuntimeError::InvalidParams("method_id".to_string()))?;
    if let AuthMethod::Terminal(method) = method {
        if !request.terminal_confirmed {
            open_visible_auth_terminal(config, method, host_bridge).await?;
            return Ok(AgentAuthenticateResult {
                agent_id: request.agent_id,
                method_id: request.method_id,
                status: AgentAuthenticateStatus::AwaitingUser,
            });
        }
    }
    logging::info(
        "acp_authenticate_request_sent",
        serde_json::json!({
            "agent_id": request.agent_id,
            "method_id": request.method_id,
        }),
    );
    connection
        .send_request(AuthenticateRequest::new(request.method_id.clone()))
        .block_task()
        .await
        .map_err(acp_error)?;
    Ok(AgentAuthenticateResult {
        agent_id: request.agent_id,
        method_id: request.method_id,
        status: AgentAuthenticateStatus::Authenticated,
    })
}

async fn open_visible_auth_terminal(
    config: &AcpAgentConfig,
    method: &AuthMethodTerminal,
    host_bridge: &HostBridge,
) -> Result<(), RuntimeError> {
    let command = config.command.clone();
    let args = config
        .args
        .iter()
        .cloned()
        .chain(method.args.iter().cloned())
        .collect::<Vec<_>>();
    let mut env = config.env.iter().cloned().collect::<HashMap<_, _>>();
    env.extend(method.env.clone());
    let host_bridge = host_bridge.clone();
    tokio::task::spawn_blocking(move || {
        host_bridge.request_with_timeout(
            "agent/auth_terminal",
            Some(serde_json::json!({
                "command": command,
                "args": args,
                "env": env,
            })),
            None,
        )
    })
    .await
    .map_err(|error| RuntimeError::Internal(error.to_string()))??;
    Ok(())
}
