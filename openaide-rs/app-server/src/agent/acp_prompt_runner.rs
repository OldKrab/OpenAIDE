use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use crate::agent::acp_schema::SessionNotification;
use agent_client_protocol::util::MatchDispatch;
use agent_client_protocol::{Agent, Dispatch, SessionMessage};
use serde_json::json;
use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_active_prompt::{
    cancel_active_prompt, send_steering_prompt_request, ActivePrompt, PromptSettlementKind,
};
use crate::agent::acp_config_options_apply::set_task_config_option_after_prior_updates;
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host_capabilities::AcpSessionPromptMap;
use crate::agent::acp_response_boundary::take_preceding_session_updates;
use crate::agent::acp_session_catalogs::{
    attach_session_event_sink_with_catalog_snapshot, deliver_session_commands_catalog,
    deliver_session_config_catalog, deliver_session_metadata_update, session_catalogs_from_update,
    session_with_catalog_snapshots, DispatchSessionCatalogs, PendingSessionCatalogs,
};
use crate::agent::acp_session_termination::close_active_session;
use crate::agent::acp_session_termination::delete_active_session;
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::attached_native_session::{AcpSessionCommand, AcpSessionConfigCommand};
use crate::agent::prompt_content::PromptContentPolicy;
use crate::agent::{
    AgentEventSink, AgentPrompt, AgentPromptOutcome, AgentSession, AgentSessionEventSink,
};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentCommandsCatalog, ConfigOptionsCatalog};

pub(super) struct PromptRunContext<'a> {
    pub(super) agent_id: &'a str,
    pub(super) supports_session_close: bool,
    pub(super) supports_session_delete: bool,
    pub(super) current_prompts: &'a AcpSessionPromptMap,
    pub(super) trace: Option<AcpTraceSession>,
    pub(super) content_policy: PromptContentPolicy,
}

// The runner coordinates independent ACP channels owned by its caller. Keep
// those lifecycle inputs explicit until they have a cohesive shared owner.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_prompt(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    cancel_rx: &mut tokio_mpsc::UnboundedReceiver<()>,
    close_rx: &mut tokio_mpsc::UnboundedReceiver<mpsc::Sender<Result<(), RuntimeError>>>,
    context: PromptRunContext<'_>,
    prompt: AgentPrompt,
    sink: Arc<dyn AgentEventSink>,
    request_guard: crate::agent::attached_native_session::PromptRequestGuard,
    command_rx: &mut tokio_mpsc::UnboundedReceiver<AcpSessionCommand>,
    config_rx: &mut tokio_mpsc::UnboundedReceiver<AcpSessionConfigCommand>,
    config_catalog: &mut ConfigOptionsCatalog,
    commands_catalog: &mut Option<AgentCommandsCatalog>,
    session_snapshot: &AgentSession,
    session_event_sink: &mut Option<Arc<dyn AgentSessionEventSink>>,
    session_projection: &mut Option<LivePromptProjection>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<AgentPromptOutcome, RuntimeError> {
    let prompt_started_at = Instant::now();
    if prompt.cancellation.is_cancelled() {
        return Ok(AgentPromptOutcome::Cancelled);
    }
    let active_session_id = active_session.session_id().to_string();
    crate::logging::info(
        "acp_prompt_dispatch_started",
        json!({
            "agent_id": context.agent_id,
            "task_id": prompt.task_id.as_str(),
            "session_id": active_session_id.as_str(),
        }),
    );
    while cancel_rx.try_recv().is_ok() {}
    let (preceding_update_drain_tx, mut preceding_update_drain_rx) =
        tokio::sync::mpsc::unbounded_channel();
    let mut active_prompt = ActivePrompt::start(
        active_session,
        context.current_prompts,
        context.agent_id,
        context.content_policy,
        context.trace.as_ref(),
        prompt,
        sink.clone(),
        session_projection.as_ref(),
        request_guard,
        Some(preceding_update_drain_tx),
    )?;

    let mut cancel_sent = false;
    let mut cancel_requested_at = None;
    let mut settled_by_response = false;
    let result = loop {
        if active_prompt.cancellation().is_cancelled() && !cancel_sent {
            // A session cancel can arrive without cancelling the prompt token.
            active_prompt.mark_cancel_requested();
            match dispatch_prompt_cancel(
                active_session,
                context.trace.as_ref(),
                context.agent_id,
                active_prompt.task_id(),
                active_session_id.as_str(),
                "turn_token",
            )
            .await
            {
                Ok(requested_at) => cancel_requested_at = Some(requested_at),
                Err(error) => break Err(error),
            }
            cancel_sent = true;
        }
        tokio::select! {
            Some(()) = cancel_rx.recv(), if !cancel_sent => {
                // Keep primary `cancelled` terminal for explicit session cancellation,
                // even when a steer was admitted first.
                active_prompt.mark_cancel_requested();
                match dispatch_prompt_cancel(
                    active_session,
                    context.trace.as_ref(),
                    context.agent_id,
                    active_prompt.task_id(),
                    active_session_id.as_str(),
                    "session_channel",
                ).await {
                    Ok(requested_at) => cancel_requested_at = Some(requested_at),
                    Err(error) => {
                        break Err(error);
                    }
                }
                cancel_sent = true;
            }
            close = close_rx.recv() => {
                let Some(reply_tx) = close else {
                    break Err(RuntimeError::NotReady("ACP close channel stopped".to_string()));
                };
                if !context.supports_session_close && !cancel_sent {
                    let _ = dispatch_prompt_cancel(
                        active_session,
                        context.trace.as_ref(),
                        context.agent_id,
                        active_prompt.task_id(),
                        active_session_id.as_str(),
                        "session_close_fallback",
                    ).await;
                }
                let connection = active_session.connection();
                close_active_session(
                    connection,
                    active_session.session_id().clone(),
                    context.supports_session_close,
                    context.trace.as_ref(),
                )
                .await;
                let _ = reply_tx.send(Ok(()));
                logging::warn(
                    "acp_prompt_session_closed",
                    json!({
                        "agent_id": context.agent_id,
                        "task_id": active_prompt.task_id(),
                        "active_session_id": active_session_id.as_str(),
                    }),
                );
                break Err(RuntimeError::NotReady("ACP session closed".to_string()));
            }
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break Err(RuntimeError::NotReady("ACP command channel stopped".to_string()));
                };
                match command {
                    AcpSessionCommand::Snapshot { reply_tx } => {
                        let _ = reply_tx.send(Ok(session_with_catalog_snapshots(
                            session_snapshot,
                            config_catalog,
                            commands_catalog,
                        )));
                    }
                    AcpSessionCommand::SetEventSink { sink } => {
                        let config_snapshot = (config_catalog.status
                            != crate::protocol::model::ConfigOptionsStatus::Empty
                            && !config_catalog.agent_id.is_empty())
                            .then_some(&*config_catalog);
                        attach_session_event_sink_with_catalog_snapshot(
                            session_event_sink,
                            pending_session_catalogs,
                            config_snapshot,
                            commands_catalog.as_ref(),
                            sink,
                        )?;
                        *session_projection = session_event_sink.as_ref().map(|sink| {
                            LivePromptProjection::for_session(context.agent_id, sink.clone())
                        });
                    }
                    AcpSessionCommand::Load { reply_tx, .. } => {
                        let _ = reply_tx.send(Err(RuntimeError::NotReady(
                            "ACP session already has an active prompt".to_string(),
                        )));
                    }
                    AcpSessionCommand::Prompt { done_tx, .. } => {
                        let _ = done_tx.send(Err(RuntimeError::NotReady(
                            "ACP session already has an active prompt".to_string(),
                        )));
                    }
                    AcpSessionCommand::Steer {
                        prompt,
                        request_guard,
                    } => {
                        if let Err(error) = send_steering_prompt_request(
                            active_session,
                            prompt,
                            context.content_policy,
                            context.trace.as_ref(),
                            Some(active_prompt.steering_settlement()),
                            Some(sink.clone()),
                            request_guard,
                        ) {
                            logging::error(
                                "acp_steering_prompt_start_failed",
                                json!({ "error": error.to_string() }),
                            );
                        }
                    }
                    AcpSessionCommand::Delete { reply_tx } => {
                        let connection = active_session.connection();
                        let result = delete_active_session(
                            connection,
                            active_session.session_id().clone(),
                            context.supports_session_delete,
                            context.trace.as_ref(),
                        )
                        .await;
                        let _ = reply_tx.send(result);
                        break Err(RuntimeError::NotReady("ACP session deleted".to_string()));
                    }
                }
            }
            config = config_rx.recv() => {
                let Some(config) = config else {
                    break Err(RuntimeError::NotReady("ACP config channel stopped".to_string()));
                };
                handle_prompt_config_command(
                    active_session,
                    config_catalog,
                    commands_catalog,
                    session_projection.clone(),
                    session_event_sink.clone(),
                    pending_session_catalogs,
                    config,
                )
                .await?;
            }
            completion = active_prompt.next_completion() => {
                let Some(completion) = completion else {
                    break Err(RuntimeError::NotReady("ACP prompt completion channel stopped".to_string()));
                };
                project_preceding_session_updates(
                    active_session,
                    context.agent_id,
                    active_prompt.task_id(),
                    active_session_id.as_str(),
                    session_projection.clone(),
                    session_event_sink.clone(),
                    pending_session_catalogs,
                    config_catalog,
                    commands_catalog,
                )
                .await?;
                active_prompt.mark_settled(PromptSettlementKind::PromptResponse);
                settled_by_response = true;
                let result = completion.finish();
                let succeeded = result.is_ok();
                logging::info(
                    "acp_prompt_result",
                    json!({
                        "agent_id": context.agent_id,
                        "task_id": active_prompt.task_id(),
                        "active_session_id": active_session_id.as_str(),
                        "result": if succeeded { "stop_reason" } else { "error" },
                    }),
                );
                break result;
            }
            Some(reply_tx) = preceding_update_drain_rx.recv() => {
                let result = project_preceding_session_updates(
                    active_session,
                    context.agent_id,
                    active_prompt.task_id(),
                    active_session_id.as_str(),
                    session_projection.clone(),
                    session_event_sink.clone(),
                    pending_session_catalogs,
                    config_catalog,
                    commands_catalog,
                )
                .await;
                let _ = reply_tx.send(result);
            }
            update = active_session.read_update() => {
                let update = match update.map_err(acp_error) {
                    Ok(update) => update,
                    Err(error) => break Err(error),
                };
                match apply_prompt_session_message(
                    context.agent_id,
                    active_prompt.task_id(),
                    active_session_id.as_str(),
                    update,
                    session_projection.clone(),
                    session_event_sink.clone(),
                    pending_session_catalogs,
                ).await {
                    Ok(catalogs) => {
                        apply_session_catalogs(catalogs, config_catalog, commands_catalog)
                    }
                    Err(error) => break Err(error),
                }
            }
        }
    };

    // Retire every still-pending response from lifecycle ownership. The session-level
    // update consumer remains attached and continues accepting late updates.
    active_prompt.mark_settled(PromptSettlementKind::RunnerExit);

    if let (Some(trace), Some(requested_at)) = (context.trace.as_ref(), cancel_requested_at) {
        trace.record_value(
            "runtime",
            "session/cancel.prompt_settled",
            json!({
                "taskId": active_prompt.task_id(),
                "sessionId": active_session_id.as_str(),
                "elapsed_ms": requested_at.elapsed().as_millis(),
                "result": runtime_result_name(&result),
            }),
        );
    }
    logging::info(
        "acp_prompt_finish",
        json!({
            "agent_id": context.agent_id,
            "task_id": active_prompt.task_id(),
            "active_session_id": active_session_id.as_str(),
            "settlement_kind": if settled_by_response { "prompt_response" } else { "runner_exit" },
            "result_status": runtime_result_name(&result),
            "error_kind": result.as_ref().err().map(RuntimeError::reason),
            "error_code": result.as_ref().err().map(RuntimeError::code),
            "duration_ms": prompt_started_at.elapsed().as_millis(),
            "cancel_to_settlement_ms": cancel_requested_at
                .map(|started: Instant| started.elapsed().as_millis()),
        }),
    );
    result
}

async fn dispatch_prompt_cancel(
    active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
    trace: Option<&AcpTraceSession>,
    agent_id: &str,
    task_id: &str,
    active_session_id: &str,
    source: &str,
) -> Result<Instant, RuntimeError> {
    let requested_at = Instant::now();
    if let Some(trace) = trace {
        trace.record_value(
            "runtime",
            "session/cancel.worker_received",
            json!({
                "taskId": task_id,
                "sessionId": active_session_id,
                "source": source,
            }),
        );
    }
    logging::warn(
        "acp_prompt_cancel_requested",
        json!({
            "agent_id": agent_id,
            "task_id": task_id,
            "active_session_id": active_session_id,
            "source": source,
            "boundary": "attached_native_session",
        }),
    );
    let result = cancel_active_prompt(active_session, trace).await;
    let dispatch_ms = requested_at.elapsed().as_millis();
    match &result {
        Ok(()) => logging::info(
            "acp_prompt_cancel_dispatch_completed",
            json!({
                "agent_id": agent_id,
                "task_id": task_id,
                "active_session_id": active_session_id,
                "source": source,
                "boundary": "acp_connection",
                "dispatch_ms": dispatch_ms,
            }),
        ),
        Err(error) => logging::error(
            "acp_prompt_cancel_send_failed",
            json!({
                "agent_id": agent_id,
                "task_id": task_id,
                "active_session_id": active_session_id,
                "source": source,
                "boundary": "acp_connection",
                "dispatch_ms": dispatch_ms,
                "error_code": error.code(),
                "error_kind": error.reason(),
            }),
        ),
    }
    result.map(|()| requested_at)
}

#[allow(clippy::too_many_arguments)]
async fn project_preceding_session_updates(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    agent_id: &str,
    task_id: &str,
    active_session_id: &str,
    projection: Option<LivePromptProjection>,
    session_event_sink: Option<Arc<dyn AgentSessionEventSink>>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
    config_catalog: &mut ConfigOptionsCatalog,
    commands_catalog: &mut Option<AgentCommandsCatalog>,
) -> Result<(), RuntimeError> {
    for update in take_preceding_session_updates(active_session).await? {
        let catalogs = apply_prompt_session_message(
            agent_id,
            task_id,
            active_session_id,
            update,
            projection.clone(),
            session_event_sink.clone(),
            pending_session_catalogs,
        )
        .await?;
        apply_session_catalogs(catalogs, config_catalog, commands_catalog);
    }
    Ok(())
}

async fn apply_prompt_session_message(
    agent_id: &str,
    task_id: &str,
    active_session_id: &str,
    update: SessionMessage,
    projection: Option<LivePromptProjection>,
    session_event_sink: Option<Arc<dyn AgentSessionEventSink>>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<DispatchSessionCatalogs, RuntimeError> {
    match update {
        SessionMessage::SessionMessage(dispatch) => {
            dispatch_session_notification(
                agent_id,
                dispatch,
                projection,
                session_event_sink,
                pending_session_catalogs,
            )
            .await
        }
        SessionMessage::StopReason(_) => {
            logging::info(
                "acp_prompt_update_stop_reason",
                json!({
                    "agent_id": agent_id,
                    "task_id": task_id,
                    "active_session_id": active_session_id,
                }),
            );
            Ok(DispatchSessionCatalogs::default())
        }
        _ => Ok(DispatchSessionCatalogs::default()),
    }
}

fn runtime_result_name(result: &Result<AgentPromptOutcome, RuntimeError>) -> &'static str {
    match result {
        Ok(AgentPromptOutcome::EndTurn) => "end_turn",
        Ok(AgentPromptOutcome::MaxTokens) => "max_tokens",
        Ok(AgentPromptOutcome::MaxTurnRequests) => "max_turn_requests",
        Ok(AgentPromptOutcome::Refusal) => "refusal",
        Ok(AgentPromptOutcome::Cancelled) => "cancelled",
        Ok(AgentPromptOutcome::Other(_)) => "other",
        Err(_) => "error",
    }
}

async fn handle_prompt_config_command(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    catalog: &mut ConfigOptionsCatalog,
    commands_catalog: &mut Option<AgentCommandsCatalog>,
    projection: Option<LivePromptProjection>,
    session_event_sink: Option<Arc<dyn AgentSessionEventSink>>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
    command: AcpSessionConfigCommand,
) -> Result<(), RuntimeError> {
    match command {
        AcpSessionConfigCommand::SetConfigOption {
            agent_id,
            session_id,
            config_id,
            value,
            operation_id,
            queued_at,
            reply_tx,
        } => {
            logging::info(
                "acp_config_option_command_received",
                json!({
                    "session_id": session_id,
                    "operation_id": operation_id,
                    "queue_wait_ms": queued_at.elapsed().as_millis(),
                }),
            );
            let connection = active_session.connection().clone();
            let mut response = match set_task_config_option_after_prior_updates(
                &connection,
                active_session,
                config_id,
                value,
                &agent_id,
                &operation_id,
            )
            .await
            {
                Ok(response) => response,
                Err(error) => {
                    let _ = reply_tx.send(Err(RuntimeError::Internal(error.to_string())));
                    return Err(error);
                }
            };
            for update in response.take_prior_updates() {
                let SessionMessage::SessionMessage(dispatch) = update else {
                    continue;
                };
                let catalogs = match dispatch_session_notification(
                    &agent_id,
                    dispatch,
                    projection.clone(),
                    session_event_sink.clone(),
                    pending_session_catalogs,
                )
                .await
                {
                    Ok(catalogs) => catalogs,
                    Err(error) => {
                        let _ = reply_tx.send(Err(RuntimeError::Internal(error.to_string())));
                        return Err(error);
                    }
                };
                apply_session_catalogs(catalogs, catalog, commands_catalog);
            }
            let result = response.finish_with_session_sink(session_event_sink.as_deref());
            logging::info(
                "acp_config_option_catalog_published",
                json!({
                    "session_id": session_id,
                    "operation_id": operation_id,
                    "result_status": if result.is_ok() { "ok" } else { "error" },
                }),
            );
            if let Ok(next_catalog) = &result {
                *catalog = next_catalog.clone();
            }
            let _ = reply_tx.send(result);
        }
    }
    Ok(())
}

pub(super) async fn dispatch_session_notification(
    agent_id: &str,
    dispatch: Dispatch,
    projection: Option<LivePromptProjection>,
    session_event_sink: Option<Arc<dyn AgentSessionEventSink>>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<DispatchSessionCatalogs, RuntimeError> {
    let catalogs = Arc::new(Mutex::new(DispatchSessionCatalogs::default()));
    let catalogs_sink = catalogs.clone();
    MatchDispatch::new(dispatch)
        .if_notification(async move |notification: SessionNotification| {
            *catalogs_sink
                .lock()
                .expect("ACP session catalog update lock poisoned") =
                session_catalogs_from_update(agent_id, &notification.update);
            if let Some(projection) = projection {
                projection.emit(notification.update).map_err(|error| {
                    agent_client_protocol::util::internal_error(error.to_string())
                })?;
            }
            Ok(())
        })
        .await
        .otherwise_ignore()
        .map_err(acp_error)?;
    let catalogs = std::mem::take(
        &mut *catalogs
            .lock()
            .expect("ACP session catalog update lock poisoned"),
    );
    if let Some(catalog) = catalogs.config.clone() {
        deliver_session_config_catalog(
            catalog,
            session_event_sink.as_ref(),
            pending_session_catalogs,
        )?;
    }
    if let Some(catalog) = catalogs.commands.clone() {
        deliver_session_commands_catalog(
            catalog,
            session_event_sink.as_ref(),
            pending_session_catalogs,
        )?;
    }
    if let Some(update) = catalogs.metadata.clone() {
        deliver_session_metadata_update(
            update,
            session_event_sink.as_ref(),
            pending_session_catalogs,
        )?;
    }
    Ok(catalogs)
}

fn apply_session_catalogs(
    catalogs: DispatchSessionCatalogs,
    config_catalog: &mut ConfigOptionsCatalog,
    commands_catalog: &mut Option<AgentCommandsCatalog>,
) {
    if let Some(catalog) = catalogs.config {
        *config_catalog = catalog;
    }
    if let Some(catalog) = catalogs.commands {
        *commands_catalog = Some(catalog);
    }
}
