use std::sync::{mpsc, Arc, Mutex};

use agent_client_protocol::schema::SessionNotification;
use agent_client_protocol::util::MatchDispatch;
use agent_client_protocol::{Agent, Dispatch, SessionMessage};
use serde_json::json;
use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_active_prompt::{
    cancel_active_prompt, send_steering_prompt_request, ActivePrompt,
};
use crate::agent::acp_config_options_apply::set_task_config_option_after_prior_updates;
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host_capabilities::AcpSessionPromptMap;
use crate::agent::acp_session_catalogs::{
    attach_session_event_sink_to_slot, deliver_session_commands_catalog,
    deliver_session_config_catalog, deliver_session_metadata_update, session_catalogs_from_update,
    DispatchSessionCatalogs, PendingSessionCatalogs,
};
use crate::agent::acp_session_client::{AcpSessionCommand, AcpSessionConfigCommand};
use crate::agent::acp_session_termination::close_active_session;
use crate::agent::acp_session_termination::delete_active_session;
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::prompt_content::PromptContentPolicy;
use crate::agent::{AgentEventSink, AgentPrompt, AgentPromptOutcome, AgentSessionEventSink};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;

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
    command_rx: &mut tokio_mpsc::UnboundedReceiver<AcpSessionCommand>,
    config_rx: &mut tokio_mpsc::UnboundedReceiver<AcpSessionConfigCommand>,
    config_catalog: &mut ConfigOptionsCatalog,
    session_event_sink: &mut Option<Arc<dyn AgentSessionEventSink>>,
    session_projection: &mut Option<LivePromptProjection>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<AgentPromptOutcome, RuntimeError> {
    if prompt.cancellation.is_cancelled() {
        return Ok(AgentPromptOutcome::Cancelled);
    }
    let active_session_id = active_session.session_id().to_string();
    while cancel_rx.try_recv().is_ok() {}
    let mut active_prompt = ActivePrompt::start(
        active_session,
        context.current_prompts,
        context.agent_id,
        context.content_policy,
        context.trace.as_ref(),
        prompt,
        sink,
        session_projection.as_ref(),
    )?;

    let mut cancel_sent = false;
    let result = loop {
        if active_prompt.cancellation().is_cancelled() && !cancel_sent {
            cancel_active_prompt(active_session, context.trace.as_ref()).await;
            cancel_sent = true;
        }
        tokio::select! {
            Some(()) = cancel_rx.recv(), if !cancel_sent => {
                logging::warn(
                    "acp_prompt_cancel_requested",
                    json!({
                        "agent_id": context.agent_id,
                        "task_id": active_prompt.task_id(),
                        "active_session_id": active_session_id.as_str(),
                    }),
                );
                cancel_active_prompt(active_session, context.trace.as_ref()).await;
                cancel_sent = true;
            }
            close = close_rx.recv() => {
                let Some(reply_tx) = close else {
                    break Err(RuntimeError::NotReady("ACP close channel stopped".to_string()));
                };
                if !context.supports_session_close && !cancel_sent {
                    cancel_active_prompt(active_session, context.trace.as_ref()).await;
                }
                let connection = active_session.connection();
                close_active_session(
                    &connection,
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
                    AcpSessionCommand::SetEventSink { sink } => {
                        attach_session_event_sink_to_slot(
                            session_event_sink,
                            pending_session_catalogs,
                            sink,
                        )?;
                        *session_projection = session_event_sink.as_ref().map(|sink| {
                            LivePromptProjection::for_session(context.agent_id, sink.clone())
                        });
                    }
                    AcpSessionCommand::Prompt { done_tx, .. } => {
                        let _ = done_tx.send(Err(RuntimeError::NotReady(
                            "ACP session already has an active prompt".to_string(),
                        )));
                    }
                    AcpSessionCommand::Steer { prompt } => {
                        if let Err(error) = send_steering_prompt_request(
                            active_session,
                            prompt,
                            context.content_policy,
                            context.trace.as_ref(),
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
                            &connection,
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
                    session_projection.clone(),
                    session_event_sink.clone(),
                    pending_session_catalogs,
                    config,
                )
                .await?;
            }
            completion = active_prompt.next_completion() => {
                let Some(result) = completion else {
                    break Err(RuntimeError::NotReady("ACP prompt completion channel stopped".to_string()));
                };
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
            update = active_session.read_update() => {
                let update = match update.map_err(acp_error) {
                    Ok(update) => update,
                    Err(error) => break Err(error),
                };
                match update {
                    SessionMessage::SessionMessage(dispatch) => {
                        match dispatch_session_notification(
                            context.agent_id,
                            dispatch,
                            session_projection.clone(),
                            session_event_sink.clone(),
                            pending_session_catalogs,
                        ).await {
                            Ok(Some(catalog)) => *config_catalog = catalog,
                            Ok(None) => {}
                            Err(error) => break Err(error),
                        }
                    }
                    SessionMessage::StopReason(_) => {
                        logging::info(
                            "acp_prompt_update_stop_reason",
                            json!({
                                "agent_id": context.agent_id,
                                "task_id": active_prompt.task_id(),
                                "active_session_id": active_session_id.as_str(),
                            }),
                        );
                    }
                    _ => {}
                }
            }
        }
    };

    logging::info(
        "acp_prompt_finish",
        json!({
            "agent_id": context.agent_id,
            "task_id": active_prompt.task_id(),
            "active_session_id": active_session_id.as_str(),
            "result": runtime_result_name(&result),
        }),
    );
    result
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
    projection: Option<LivePromptProjection>,
    session_event_sink: Option<Arc<dyn AgentSessionEventSink>>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
    command: AcpSessionConfigCommand,
) -> Result<(), RuntimeError> {
    match command {
        AcpSessionConfigCommand::SetConfigOption {
            agent_id,
            config_id,
            value,
            reply_tx,
        } => {
            let connection = active_session.connection();
            let mut response = match set_task_config_option_after_prior_updates(
                &connection,
                active_session,
                config_id,
                value,
                &agent_id,
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
                if let Err(error) = dispatch_session_notification(
                    &agent_id,
                    dispatch,
                    projection.clone(),
                    session_event_sink.clone(),
                    pending_session_catalogs,
                )
                .await
                {
                    let _ = reply_tx.send(Err(RuntimeError::Internal(error.to_string())));
                    return Err(error);
                }
            }
            let result = response.finish_with_session_sink(session_event_sink.as_deref());
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
) -> Result<Option<ConfigOptionsCatalog>, RuntimeError> {
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
    let mut catalogs = std::mem::take(
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
    if let Some(catalog) = catalogs.commands.take() {
        deliver_session_commands_catalog(
            catalog,
            session_event_sink.as_ref(),
            pending_session_catalogs,
        )?;
    }
    if let Some(update) = catalogs.metadata.take() {
        deliver_session_metadata_update(
            update,
            session_event_sink.as_ref(),
            pending_session_catalogs,
        )?;
    }
    Ok(catalogs.config)
}
