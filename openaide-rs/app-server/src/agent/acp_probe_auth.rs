use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::{
    AuthenticateRequest, CreateTerminalRequest, InitializeRequest, KillTerminalRequest,
    ProtocolVersion, ReadTextFileRequest, ReleaseTerminalRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SessionNotification,
    TerminalOutputRequest, WaitForTerminalExitRequest, WriteTextFileRequest,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Handled};

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::acp_agent_status::agent_probe_result_from_initialize;
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host::initialize_request;
use crate::agent::acp_host_capabilities::AcpHostCapabilityHandlers;
use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwnerId};
use crate::agent::acp_session_capabilities::{validate_auth_method, validate_initialize_protocol};
use crate::agent::AgentAuthenticateRequest;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{AgentAuthenticateResult, AgentAuthenticateStatus, AgentProbeResult};

pub(super) async fn run_agent_probe(
    config: AcpAgentConfig,
    agent_id: String,
    timeout: Duration,
    host_bridge: HostBridge,
) -> Result<AgentProbeResult, RuntimeError> {
    let agent = config.to_acp_agent(None, &host_bridge, None)?;

    let probe = Client
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
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            validate_initialize_protocol(&initialize)
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
            Ok(agent_probe_result_from_initialize(agent_id, &initialize))
        });

    tokio::time::timeout(timeout, probe)
        .await
        .map_err(|_| RuntimeError::NotReady("ACP Agent probe timed out".to_string()))?
        .map_err(acp_error)
}

pub(super) async fn run_agent_authenticate(
    config: AcpAgentConfig,
    request: AgentAuthenticateRequest,
    timeout: Duration,
    host_bridge: HostBridge,
) -> Result<AgentAuthenticateResult, RuntimeError> {
    let agent = config.to_acp_agent(None, &host_bridge, None)?;
    let agent_id = request.agent_id.clone();
    let method_id = request.method_id.clone();
    let terminal_registry = AcpHostTerminalRegistry::new(host_bridge.clone());
    let terminal_owner_id = AcpTerminalOwnerId::next();
    terminal_registry.begin_open(terminal_owner_id);
    let terminal_owner = terminal_registry.owner(terminal_owner_id);
    let host_capabilities = AcpHostCapabilityHandlers::new(
        host_bridge.clone(),
        None,
        Arc::new(Mutex::new(HashMap::new())),
        terminal_registry,
        Arc::default(),
        Arc::default(),
    );

    let auth = Client
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
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: ReadTextFileRequest, responder, _connection| {
                    responder.respond(host_capabilities.read_text_file(request).await?)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: WriteTextFileRequest, responder, _connection| {
                    responder.respond(host_capabilities.write_text_file(request).await?)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: CreateTerminalRequest, responder, _connection| {
                    responder.respond(host_capabilities.create_terminal(request).await?)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: TerminalOutputRequest, responder, _connection| {
                    responder.respond(host_capabilities.terminal_output(request).await?)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: WaitForTerminalExitRequest, responder, _connection| {
                    responder.respond(host_capabilities.wait_for_terminal_exit(request).await?)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: KillTerminalRequest, responder, _connection| {
                    responder.respond(host_capabilities.kill_terminal(request).await?)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: ReleaseTerminalRequest, responder, _connection| {
                    responder.respond(host_capabilities.release_terminal(request).await?)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
            let initialize = connection
                .send_request(initialize_request(&host_bridge))
                .block_task()
                .await?;
            validate_initialize_protocol(&initialize)
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
            validate_auth_method(&initialize, &method_id)
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
            connection
                .send_request(AuthenticateRequest::new(method_id.clone()))
                .block_task()
                .await?;
            Ok(AgentAuthenticateResult {
                agent_id,
                method_id,
                status: AgentAuthenticateStatus::Authenticated,
            })
        });

    let result = tokio::time::timeout(timeout, auth)
        .await
        .map_err(|_| RuntimeError::NotReady("ACP Agent authentication timed out".to_string()))?
        .map_err(acp_error);
    let cleanup = tokio::task::spawn_blocking(move || terminal_owner.close())
        .await
        .map_err(|error| RuntimeError::Internal(error.to_string()))?;
    match result {
        Ok(authenticated) => cleanup.map(|()| authenticated),
        Err(error) => Err(error),
    }
}
