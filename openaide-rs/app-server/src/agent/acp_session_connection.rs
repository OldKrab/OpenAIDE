use crate::agent::acp_schema::{
    CreateTerminalRequest, KillTerminalRequest, ReadTextFileRequest, ReleaseTerminalRequest,
    RequestPermissionRequest, SessionNotification, SessionUpdate, TerminalOutputRequest,
    WaitForTerminalExitRequest, WriteTextFileRequest,
};
use agent_client_protocol::{
    Agent, Client, ConnectTo, ConnectionTo, Handled, JsonRpcMessage, UntypedMessage,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::agent::acp_background_work::{
    route_async_task_update, AsyncTaskRouting, SESSION_STATE_METHOD,
};
use crate::agent::acp_draft_subagents::{decode_draft_subagent_update, DraftSubagentNotification};
use crate::agent::acp_early_session_catalogs::EarlySessionCatalogs;
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

pub(super) type AcpActiveSessionIds =
    std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>;

pub(super) struct AcpSessionConnectionContext {
    pub(super) agent_id: String,
    pub(super) host_bridge: HostBridge,
    pub(super) trace: Option<AcpTraceSession>,
    pub(super) current_prompts: crate::agent::acp_host_capabilities::AcpSessionPromptMap,
    pub(super) load_replay: LoadReplayCaptures,
    /// Sessions this connection has attached. It tells an update for a session that is
    /// still opening from one a session worker will receive.
    pub(super) active_session_ids: AcpActiveSessionIds,
    pub(super) early_catalogs: EarlySessionCatalogs,
    pub(super) terminal_registry: AcpHostTerminalRegistry,
    pub(super) session_event_sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap,
    pub(super) session_traces: crate::agent::acp_host_capabilities::AcpSessionTraceMap,
    pub(super) elicitation_cancellations:
        crate::agent::acp_host_capabilities::AcpElicitationCancellationMap,
    pub(super) native_subagents: AcpNativeSubagentRouter,
    /// Receives the Account Limits the Agent pushes on the connection.
    pub(super) agent_reports: Option<crate::agent::status_cache::AgentReportRecorder>,
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
    let notification_agent_id = context.agent_id.clone();
    let notification_active_session_ids = context.active_session_ids;
    let notification_early_catalogs = context.early_catalogs;
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
    let state_trace = notification_trace.clone();
    let state_session_traces = notification_session_traces.clone();
    let notification_load_replay = context.load_replay;
    let raw_load_replay = notification_load_replay.clone();
    let raw_native_subagents = context.native_subagents.clone();
    let notification_native_subagents = context.native_subagents;
    let agent_reports = context.agent_reports;
    let raw_agent_id = notification_agent_id.clone();
    let undecodable_kinds = Arc::new(Mutex::new(HashSet::<String>::new()));

    // ACP request callbacks run inside the shared connection's dispatch loop. Every host wait
    // must be spawned so one session cannot block updates and responses for every other session.
    Client
        .builder()
        .name("openaide")
        .on_receive_notification(
            async move |notification: UntypedMessage, cx| {
                if notification.method()
                    == crate::agent::acp_account_limits_projection::ACCOUNT_LIMITS_KEY
                {
                    crate::agent::acp_account_limits_read::record_pushed_account_limits(
                        agent_reports.as_ref(),
                        notification.params(),
                    );
                    return Ok(Handled::Yes);
                }
                if notification.method() == crate::agent::acp_auth_status::AUTH_STATUS_UPDATE_METHOD
                {
                    crate::agent::acp_auth_status::record_pushed_auth_status(
                        agent_reports.as_ref(),
                        notification.params(),
                    );
                    return Ok(Handled::Yes);
                }
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
                if let Some(draft) = decode_draft_subagent_update(&notification) {
                    handle_draft_subagent_notification(
                        &notification,
                        draft,
                        &state_trace,
                        &state_session_traces,
                        &raw_load_replay,
                        &raw_native_subagents,
                    );
                    return Ok(Handled::Yes);
                }
                trace_session_state_change(&notification, &state_trace, &state_session_traces);
                let notification = match route_async_task_update(&notification) {
                    AsyncTaskRouting::NotAsyncTask => notification,
                    AsyncTaskRouting::Forward(state_change) => state_change,
                    AsyncTaskRouting::Drop => return Ok(Handled::Yes),
                };
                if session_update_is_undecodable(&notification, &raw_agent_id, &undecodable_kinds) {
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
                // Child sessions were routed above; only a root session can be opening.
                let retry = defer_catalog_update_for_unattached_session(
                    &notification_agent_id,
                    &notification,
                    &notification_active_session_ids,
                    &notification_early_catalogs,
                );
                Ok(unhandled_session_update(notification, cx, retry))
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

/// A state report is not a `session/update`, so the typed handler below never
/// traces it. Without it a trace cannot show a cycle the Agent started itself.
fn trace_session_state_change(
    notification: &UntypedMessage,
    trace: &Option<AcpTraceSession>,
    session_traces: &crate::agent::acp_host_capabilities::AcpSessionTraceMap,
) {
    if notification.method() != SESSION_STATE_METHOD {
        return;
    }
    let owning_trace = notification
        .params()
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .and_then(|session_id| {
            session_traces
                .lock()
                .expect("ACP session trace map lock poisoned")
                .get(session_id)
                .cloned()
        })
        .or_else(|| trace.clone());
    if let Some(trace) = owning_trace {
        trace.record_value(
            "agent_to_client",
            SESSION_STATE_METHOD,
            notification.params().clone(),
        );
    }
}

fn handle_session_update_notification(
    notification: SessionNotification,
    trace: &Option<AcpTraceSession>,
    session_traces: &crate::agent::acp_host_capabilities::AcpSessionTraceMap,
    load_replay: &LoadReplayCaptures,
) -> Option<SessionNotification> {
    log_tool_call_status_received(&notification);
    let session_id = notification.session_id.to_string();
    let announced_child = match &notification.update {
        SessionUpdate::SubagentUpdate(update) => Some(update.session_id.to_string()),
        _ => None,
    };
    let announced_child = announced_child.as_deref();
    if let Some(trace) = session_trace(&session_id, announced_child, trace, session_traces) {
        trace.record("agent_to_client", "session/update", &notification);
    }
    if absorbed_by_load_replay(
        load_replay,
        &session_id,
        announced_child,
        Some(&notification.update),
    ) {
        return None;
    }
    Some(notification)
}

/// Draft Subagent updates have no type in the released schema. They take the
/// same trace and load-replay path as a typed update before the router applies them.
fn handle_draft_subagent_notification(
    notification: &UntypedMessage,
    draft: Result<DraftSubagentNotification, serde_json::Error>,
    trace: &Option<AcpTraceSession>,
    session_traces: &crate::agent::acp_host_capabilities::AcpSessionTraceMap,
    load_replay: &LoadReplayCaptures,
    native_subagents: &AcpNativeSubagentRouter,
) {
    let draft = match draft {
        Ok(draft) => draft,
        Err(error) => {
            crate::logging::warn(
                "acp_subagent_update_rejected",
                serde_json::json!({
                    "form": "draft",
                    "error_kind": "malformed",
                    "error_category": format!("{:?}", error.classify()),
                }),
            );
            return;
        }
    };
    let announced_child = draft.announced_child();
    if let Some(trace) = session_trace(&draft.session_id, announced_child, trace, session_traces) {
        trace.record("agent_to_client", "session/update", notification.params());
    }
    if absorbed_by_load_replay(load_replay, &draft.session_id, announced_child, None) {
        return;
    }
    if let Err(error) = native_subagents.route_draft(draft) {
        crate::logging::warn(
            "acp_subagent_update_rejected",
            serde_json::json!({
                "form": "draft",
                "error_kind": error.code(),
            }),
        );
    }
}

/// The trace that records `session_id`'s traffic. A child session never opens
/// through this client, so the child a session announces inherits that
/// session's trace. Otherwise its traffic falls back to whichever Task first
/// opened the shared Agent process.
fn session_trace(
    session_id: &str,
    announced_child: Option<&str>,
    trace: &Option<AcpTraceSession>,
    session_traces: &crate::agent::acp_host_capabilities::AcpSessionTraceMap,
) -> Option<AcpTraceSession> {
    let mut session_traces = session_traces
        .lock()
        .expect("ACP session trace map lock poisoned");
    let owning_trace = session_traces.get(session_id).cloned();
    if let (Some(parent_trace), Some(child_session_id)) = (&owning_trace, announced_child) {
        session_traces.insert(child_session_id.to_string(), parent_trace.clone());
    }
    owning_trace.or_else(|| trace.clone())
}

/// Lets an active `session/load` capture absorb one update of the loaded
/// session or of a child it announced. Only a root `update` joins the replay,
/// including the root's `subagent_update`s, whose labels and states the
/// replayed delegation rows need; child streams stay out of the main
/// projection. Returns whether it was absorbed.
fn absorbed_by_load_replay(
    load_replay: &LoadReplayCaptures,
    session_id: &str,
    announced_child: Option<&str>,
    update: Option<&SessionUpdate>,
) -> bool {
    let mut active = load_replay
        .lock()
        .expect("ACP load replay capture lock poisoned");
    let Some(capture) = active.values_mut().find(|capture| {
        *capture.session_id.0 == *session_id || capture.subagent_session_ids.contains(session_id)
    }) else {
        return false;
    };
    if let Some(child_session_id) = announced_child {
        capture
            .subagent_session_ids
            .insert(child_session_id.to_string());
    }
    if let Some(update) = update.filter(|_| *capture.session_id.0 == *session_id) {
        capture.updates.push(update.clone());
    }
    true
}

/// An Agent may publish a session's catalogs before this client finishes attaching the
/// session it just opened. No session handler consumes such an update yet. Returns whether
/// the dispatch loop should defer it for that handler, and records every update that is lost.
fn defer_catalog_update_for_unattached_session(
    agent_id: &str,
    notification: &SessionNotification,
    active_session_ids: &AcpActiveSessionIds,
    early_catalogs: &EarlySessionCatalogs,
) -> bool {
    let update_kind = match &notification.update {
        SessionUpdate::AvailableCommandsUpdate(_) => "available_commands",
        // The open response carries the session's options, so an early copy is not kept.
        SessionUpdate::ConfigOptionUpdate(_) => "config_options",
        _ => return false,
    };
    let session_id = notification.session_id.to_string();
    if active_session_ids
        .lock()
        .expect("ACP active session id set poisoned")
        .contains(&session_id)
    {
        return false;
    }
    let deferred =
        update_kind == "available_commands" && early_catalogs.defer_commands(&session_id);
    let fields = serde_json::json!({
        "agent_id": agent_id,
        "session_id": session_id,
        "update_kind": update_kind,
        "outcome": if deferred { "deferred" } else { "dropped" },
    });
    if deferred {
        crate::logging::info("acp_session_catalog_update_unattached", fields);
    } else {
        crate::logging::warn("acp_session_catalog_update_unattached", fields);
    }
    deferred
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

/// Leaves the update to the session handlers. `retry` asks the dispatch loop to hold an
/// update none of them accepts and offer it to the next handler that is registered.
/// The typed handler would reject a `session/update` it cannot decode without
/// any trace, which once hid a whole new update form. Reports each such kind
/// once per connection and drops the update. Returns whether it was dropped.
fn session_update_is_undecodable(
    notification: &UntypedMessage,
    agent_id: &str,
    reported_kinds: &Mutex<HashSet<String>>,
) -> bool {
    if notification.method() != "session/update" {
        return false;
    }
    let Err(error) = SessionNotification::deserialize(notification.params()) else {
        return false;
    };
    let kind = notification
        .params()
        .get("update")
        .and_then(|update| update.get("sessionUpdate"))
        .and_then(serde_json::Value::as_str)
        .filter(|kind| {
            kind.len() <= 64
                && kind
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        })
        .unwrap_or("unreadable");
    if reported_kinds
        .lock()
        .expect("ACP undecodable update set poisoned")
        .insert(kind.to_string())
    {
        crate::logging::warn(
            "acp_session_update_undecodable",
            serde_json::json!({
                "agent_id": agent_id,
                "update_kind": kind,
                "error_category": format!("{:?}", error.classify()),
            }),
        );
    }
    true
}

fn unhandled_session_update<Cx>(
    notification: SessionNotification,
    cx: Cx,
    retry: bool,
) -> Handled<(SessionNotification, Cx)> {
    Handled::No {
        message: (notification, cx),
        retry,
    }
}

#[cfg(test)]
#[path = "acp_session_connection_tests.rs"]
mod tests;
