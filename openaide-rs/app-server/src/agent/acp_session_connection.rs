use agent_client_protocol::schema::{
    CreateTerminalRequest, KillTerminalRequest, ReadTextFileRequest, ReleaseTerminalRequest,
    RequestPermissionRequest, SessionNotification, TerminalOutputRequest,
    WaitForTerminalExitRequest, WriteTextFileRequest,
};
use agent_client_protocol::{Agent, Client, ConnectTo, ConnectionTo, Handled};

use crate::agent::acp_elicitation_wire::{
    CancelRequestNotification, ElicitationCreateRequest, WireRequestId,
};
use crate::agent::acp_host_capabilities::AcpHostCapabilityHandlers;
use crate::agent::acp_host_terminal_ownership::AcpHostTerminalRegistry;
use crate::agent::acp_session_lifecycle::LoadReplayCaptures;
use crate::agent::acp_trace::AcpTraceSession;
use crate::protocol::host::HostBridge;

pub(super) struct AcpSessionConnectionContext {
    pub(super) host_bridge: HostBridge,
    pub(super) trace: Option<AcpTraceSession>,
    pub(super) current_prompts: crate::agent::acp_host_capabilities::AcpSessionPromptMap,
    pub(super) load_replay: LoadReplayCaptures,
    pub(super) terminal_registry: AcpHostTerminalRegistry,
    pub(super) session_event_sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap,
    pub(super) elicitation_cancellations:
        crate::agent::acp_host_capabilities::AcpElicitationCancellationMap,
}

pub(super) async fn connect_acp_session_client<R, AgentTransport>(
    agent: AgentTransport,
    context: AcpSessionConnectionContext,
    run: impl AsyncFnOnce(ConnectionTo<Agent>) -> agent_client_protocol::Result<R>,
) -> agent_client_protocol::Result<R>
where
    AgentTransport: ConnectTo<Client>,
{
    let host_capabilities = AcpHostCapabilityHandlers::new(
        context.host_bridge,
        context.trace.clone(),
        context.current_prompts,
        context.terminal_registry,
        context.session_event_sinks,
        context.elicitation_cancellations,
    );
    let notification_trace = context.trace;
    let notification_load_replay = context.load_replay;

    // ACP request callbacks run inside the shared connection's dispatch loop. Every host wait
    // must be spawned so one session cannot block updates and responses for every other session.
    Client
        .builder()
        .name("openaide")
        .on_receive_notification(
            async move |notification: SessionNotification, cx| {
                match handle_session_update_notification(
                    notification,
                    &notification_trace,
                    &notification_load_replay,
                ) {
                    Some(notification) => Ok(unhandled_session_update(notification, cx)),
                    None => Ok(Handled::Yes),
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: ElicitationCreateRequest, responder, connection| {
                    let request_id: WireRequestId = serde_json::from_value(responder.id())
                        .map_err(|_| agent_client_protocol::Error::invalid_request())?;
                    connection.spawn({
                        let host_capabilities = host_capabilities.clone();
                        async move {
                            responder.respond_with_result(
                                host_capabilities
                                    .create_elicitation(request_id, request)
                                    .await,
                            )
                        }
                    })?;
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let host_capabilities = host_capabilities.clone();
                async move |notification: CancelRequestNotification, _connection| {
                    host_capabilities.cancel_elicitation(&notification.request_id);
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: RequestPermissionRequest, responder, connection| {
                    connection.spawn({
                        let host_capabilities = host_capabilities.clone();
                        async move {
                            responder.respond_with_result(
                                host_capabilities.request_permission(request).await,
                            )
                        }
                    })?;
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: ReadTextFileRequest, responder, connection| {
                    connection.spawn({
                        let host_capabilities = host_capabilities.clone();
                        async move {
                            responder.respond_with_result(
                                host_capabilities.read_text_file(request).await,
                            )
                        }
                    })?;
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: WriteTextFileRequest, responder, connection| {
                    connection.spawn({
                        let host_capabilities = host_capabilities.clone();
                        async move {
                            responder.respond_with_result(
                                host_capabilities.write_text_file(request).await,
                            )
                        }
                    })?;
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: CreateTerminalRequest, responder, connection| {
                    connection.spawn({
                        let host_capabilities = host_capabilities.clone();
                        async move {
                            responder.respond_with_result(
                                host_capabilities.create_terminal(request).await,
                            )
                        }
                    })?;
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: TerminalOutputRequest, responder, connection| {
                    connection.spawn({
                        let host_capabilities = host_capabilities.clone();
                        async move {
                            responder.respond_with_result(
                                host_capabilities.terminal_output(request).await,
                            )
                        }
                    })?;
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: WaitForTerminalExitRequest, responder, connection| {
                    connection.spawn({
                        let host_capabilities = host_capabilities.clone();
                        async move {
                            responder.respond_with_result(
                                host_capabilities.wait_for_terminal_exit(request).await,
                            )
                        }
                    })?;
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: KillTerminalRequest, responder, connection| {
                    connection.spawn({
                        let host_capabilities = host_capabilities.clone();
                        async move {
                            responder
                                .respond_with_result(host_capabilities.kill_terminal(request).await)
                        }
                    })?;
                    Ok(Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ReleaseTerminalRequest, responder, connection| {
                connection.spawn({
                    let host_capabilities = host_capabilities.clone();
                    async move {
                        responder
                            .respond_with_result(host_capabilities.release_terminal(request).await)
                    }
                })?;
                Ok(Handled::Yes)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, run)
        .await
}

fn handle_session_update_notification(
    notification: SessionNotification,
    trace: &Option<AcpTraceSession>,
    load_replay: &LoadReplayCaptures,
) -> Option<SessionNotification> {
    if let Some(trace) = trace {
        trace.record("agent_to_client", "session/update", &notification);
    }
    let mut active = load_replay
        .lock()
        .expect("ACP load replay capture lock poisoned");
    if let Some(capture) = active.get_mut(&notification.session_id.to_string()) {
        if notification.session_id == capture.session_id {
            capture.updates.push(notification.update);
            return None;
        }
    }
    Some(notification)
}

fn unhandled_session_update<Cx>(
    notification: SessionNotification,
    cx: Cx,
) -> Handled<(SessionNotification, Cx)> {
    Handled::No {
        message: (notification, cx),
        retry: false,
    }
}

#[cfg(test)]
mod tests;
