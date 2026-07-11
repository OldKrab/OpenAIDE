use std::path::PathBuf;

use agent_client_protocol::schema::{
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SessionNotification,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Handled};

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host::initialize_request;
use crate::agent::acp_session_capabilities::{
    validate_initialize_protocol, validate_session_list_capability,
};
use crate::agent::acp_session_lifecycle::{
    agent_list_sessions_result_from_response, request_session_list,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::AgentListSessionsResult;

/// Lists Native Sessions on an initialized ACP connection without creating a session.
pub(super) async fn run_agent_session_list(
    config: AcpAgentConfig,
    agent_id: String,
    cwd: PathBuf,
    cursor: Option<String>,
    preferred_auth_method_id: Option<String>,
    host_bridge: HostBridge,
) -> Result<AgentListSessionsResult, RuntimeError> {
    let agent = config.to_acp_agent(None, &host_bridge, None)?;
    Client
        .builder()
        .name("openaide")
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(Handled::Yes),
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |_request: RequestPermissionRequest, responder, _connection| {
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
            let initialize = connection
                .send_request(initialize_request(&host_bridge))
                .block_task()
                .await?;
            validate_initialize_protocol(&initialize)
                .and_then(|()| validate_session_list_capability(&initialize))
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
            let response = request_session_list(
                &connection,
                cwd.clone(),
                cursor,
                &initialize,
                preferred_auth_method_id.as_deref(),
            )
            .await
            .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
            Ok(agent_list_sessions_result_from_response(
                agent_id, response, &cwd, None,
            ))
        })
        .await
        .map_err(acp_error)
}
