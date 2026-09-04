use crate::agent::acp_schema::{
    CreateTerminalRequest, KillTerminalRequest, ReadTextFileRequest, ReleaseTerminalRequest,
    RequestPermissionRequest, SessionNotification, SessionUpdate, TerminalOutputRequest,
    WaitForTerminalExitRequest, WriteTextFileRequest,
};
use agent_client_protocol::{
    Agent, Client, ConnectTo, ConnectionTo, Handled, JsonRpcMessage, UntypedMessage,
};
use serde::Deserialize;

use crate::agent::acp_elicitation_wire::{
    CancelRequestNotification, ElicitationCreateRequest, RawElicitationCreateRequest, WireRequestId,
};
use crate::agent::acp_host_capabilities::{AcpHostCapabilityContext, AcpHostCapabilityHandlers};
use crate::agent::acp_host_terminal_ownership::AcpHostTerminalRegistry;
use crate::agent::acp_native_subagents::{AcpNativeSubagentRouter, RoutedSessionNotification};
use crate::agent::acp_session_lifecycle::LoadReplayCaptures;
use crate::agent::acp_tool_call_projection::tool_status_name;
use crate::agent::acp_trace::AcpTraceSession;
use crate::protocol::host::HostBridge;

pub(super) struct AcpSessionConnectionContext {
    pub(super) agent_id: String,
    pub(super) host_bridge: HostBridge,
    pub(super) trace: Option<AcpTraceSession>,
    pub(super) current_prompts: crate::agent::acp_host_capabilities::AcpSessionPromptMap,
    pub(super) load_replay: LoadReplayCaptures,
    pub(super) terminal_registry: AcpHostTerminalRegistry,
    pub(super) session_event_sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap,
    pub(super) session_traces: crate::agent::acp_host_capabilities::AcpSessionTraceMap,
    pub(super) elicitation_cancellations:
        crate::agent::acp_host_capabilities::AcpElicitationCancellationMap,
    pub(super) native_subagents: AcpNativeSubagentRouter,
}

pub(super) async fn connect_acp_session_client<R, AgentTransport>(
    agent: AgentTransport,
    context: AcpSessionConnectionContext,
    run: impl AsyncFnOnce(ConnectionTo<Agent>) -> agent_client_protocol::Result<R>,
) -> agent_client_protocol::Result<R>
where
    AgentTransport: ConnectTo<Client>,
{
    let notification_session_traces = context.session_traces.clone();
    let host_capabilities = AcpHostCapabilityHandlers::new(AcpHostCapabilityContext {
        agent_id: context.agent_id,
        host_bridge: context.host_bridge,
        trace: context.trace.clone(),
        current_prompts: context.current_prompts,
        terminal_registry: context.terminal_registry,
        session_event_sinks: context.session_event_sinks,
        session_traces: context.session_traces,
        elicitation_cancellations: context.elicitation_cancellations,
        native_subagents: context.native_subagents.clone(),
    });
    let notification_trace = context.trace;
    let notification_load_replay = context.load_replay;
    let notification_native_subagents = context.native_subagents;

    // ACP request callbacks run inside the shared connection's dispatch loop. Every host wait
    // must be spawned so one session cannot block updates and responses for every other session.
    Client
        .builder()
        .name("openaide")
        .on_receive_notification(
            async move |notification: UntypedMessage, cx| {
                if let Err(reason) = raw_plan_update_is_valid(&notification) {
                    crate::logging::warn(
                        "acp_plan_update_ignored",
                        serde_json::json!({
                            "session_id": notification
                                .params()
                                .get("sessionId")
                                .and_then(serde_json::Value::as_str),
                            "reason": reason,
                        }),
                    );
                    return Ok(Handled::Yes);
                }
                Ok(Handled::No {
                    message: (notification, cx),
                    retry: false,
                })
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_notification(
            async move |notification: SessionNotification, cx| {
                if handle_session_update_notification(
                    notification.clone(),
                    &notification_trace,
                    &notification_session_traces,
                    &notification_load_replay,
                )
                .is_none()
                {
                    return Ok(Handled::Yes);
                }
                let notification = match notification_native_subagents.route(notification) {
                    Ok(RoutedSessionNotification::Root(notification)) => *notification,
                    Ok(RoutedSessionNotification::Handled) => return Ok(Handled::Yes),
                    Err(error) => {
                        crate::logging::warn(
                            "acp_subagent_update_rejected",
                            serde_json::json!({
                                "error_kind": error.code(),
                            }),
                        );
                        return Ok(Handled::Yes);
                    }
                };
                Ok(unhandled_session_update(notification, cx))
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            {
                let host_capabilities = host_capabilities.clone();
                async move |request: RawElicitationCreateRequest, responder, connection| {
                    let request_id: WireRequestId = serde_json::from_value(
                        serde_json::to_value(responder.id())
                            .map_err(|_| agent_client_protocol::Error::invalid_request())?,
                    )
                    .map_err(|_| agent_client_protocol::Error::invalid_request())?;
                    let request = match ElicitationCreateRequest::parse_message(
                        "elicitation/create",
                        &request.0,
                    ) {
                        Ok(request) => request,
                        Err(error) => {
                            host_capabilities.trace_elicitation_decode_error(
                                &request_id,
                                &request.0,
                                &error,
                            );
                            responder.respond_with_error(error)?;
                            return Ok(Handled::Yes);
                        }
                    };
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

/// ACP's tolerant schema can skip invalid list entries, so validate Plan snapshots before typed
/// decoding. Dropping the whole notification prevents a partial list from replacing good state.
fn raw_plan_update_is_valid(notification: &UntypedMessage) -> Result<(), &'static str> {
    if notification.method() != "session/update" {
        return Ok(());
    }
    let Some(update) = notification.params().get("update") else {
        return Ok(());
    };
    if update
        .get("sessionUpdate")
        .and_then(serde_json::Value::as_str)
        != Some("plan")
    {
        return Ok(());
    }
    serde_json::from_value::<RawPlanUpdate>(update.clone())
        .map(|_| ())
        .map_err(|_| "malformed ACP Plan snapshot")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlanUpdate {
    #[allow(dead_code)]
    entries: Vec<RawPlanEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlanEntry {
    #[allow(dead_code)]
    content: String,
    #[allow(dead_code)]
    priority: RawPlanPriority,
    #[allow(dead_code)]
    status: RawPlanStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawPlanPriority {
    High,
    Medium,
    Low,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawPlanStatus {
    Pending,
    InProgress,
    Completed,
}

fn handle_session_update_notification(
    notification: SessionNotification,
    trace: &Option<AcpTraceSession>,
    session_traces: &crate::agent::acp_host_capabilities::AcpSessionTraceMap,
    load_replay: &LoadReplayCaptures,
) -> Option<SessionNotification> {
    log_tool_call_status_received(&notification);
    let owning_trace = session_traces
        .lock()
        .expect("ACP session trace map lock poisoned")
        .get(&notification.session_id.to_string())
        .cloned()
        .or_else(|| trace.clone());
    if let Some(trace) = owning_trace {
        trace.record("agent_to_client", "session/update", &notification);
    }
    let mut active = load_replay
        .lock()
        .expect("ACP load replay capture lock poisoned");
    let outer_session_id = notification.session_id.to_string();
    if let Some(capture) = active.values_mut().find(|capture| {
        capture.session_id == notification.session_id
            || capture.subagent_session_ids.contains(&outer_session_id)
    }) {
        match notification.update {
            SessionUpdate::SubagentSpawned(spawned) => {
                capture
                    .subagent_session_ids
                    .insert(spawned.subagent_session_id.to_string());
            }
            SessionUpdate::SubagentStateUpdate(_)
                if notification.session_id == capture.session_id => {}
            update if notification.session_id == capture.session_id => capture.updates.push(update),
            _ => {}
        }
        return None;
    }
    Some(notification)
}

fn log_tool_call_status_received(notification: &SessionNotification) {
    let status = match &notification.update {
        SessionUpdate::ToolCall(tool_call) => Some((
            tool_call.tool_call_id.to_string(),
            tool_status_name(&tool_call.status),
        )),
        SessionUpdate::ToolCallUpdate(update) => update
            .fields
            .status
            .as_ref()
            .map(|status| (update.tool_call_id.to_string(), tool_status_name(status))),
        _ => None,
    };
    let Some((tool_call_id, tool_status)) = status else {
        return;
    };
    crate::logging::info(
        "acp_tool_call_status_received",
        serde_json::json!({
            "session_id": notification.session_id.to_string(),
            "tool_call_id": tool_call_id,
            "tool_status": tool_status,
        }),
    );
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
#[path = "acp_session_connection_tests.rs"]
mod tests;
