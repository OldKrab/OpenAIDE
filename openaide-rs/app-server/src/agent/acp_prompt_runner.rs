use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::SessionNotification;
use agent_client_protocol::util::MatchDispatch;
use agent_client_protocol::{Agent, Dispatch, SessionMessage};
use serde_json::json;
use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_concurrent_prompts::{cancel_active_prompt, ConcurrentPrompts};
use crate::agent::acp_config_options_apply::set_task_config_option_after_prior_updates;
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host_capabilities::AcpSessionPromptMap;
use crate::agent::acp_session_catalogs::{
    attach_session_event_sink_to_slot, deliver_session_metadata_update,
    session_metadata_from_update, PendingSessionCatalogs,
};
use crate::agent::acp_session_client::{AcpSessionCommand, AcpSessionConfigCommand};
use crate::agent::acp_session_termination::close_active_session;
use crate::agent::acp_session_termination::delete_active_session;
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::prompt_content::PromptContentPolicy;
use crate::agent::{AgentEventSink, AgentPrompt, AgentSessionEventSink};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;

const POST_PROMPT_UPDATE_DRAIN: Duration = Duration::from_millis(100);

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
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<(), RuntimeError> {
    if prompt.cancellation.is_cancelled() {
        return Ok(());
    }
    let active_session_id = active_session.session_id().to_string();
    while cancel_rx.try_recv().is_ok() {}
    let mut prompts = ConcurrentPrompts::start(
        active_session,
        context.current_prompts,
        context.agent_id,
        context.content_policy,
        context.trace.as_ref(),
        prompt,
        sink,
    )?;

    let mut cancel_sent = false;
    let result = loop {
        if prompts.cancellation().is_cancelled() && !cancel_sent {
            prompts.cancel();
            cancel_active_prompt(active_session, context.trace.as_ref()).await;
            cancel_sent = true;
        }
        tokio::select! {
            Some(()) = cancel_rx.recv(), if !cancel_sent => {
                prompts.cancel();
                logging::warn(
                    "acp_prompt_cancel_requested",
                    json!({
                        "agent_id": context.agent_id,
                        "task_id": prompts.task_id(),
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
                prompts.finish("ACP session closed");
                logging::warn(
                    "acp_prompt_session_closed",
                    json!({
                        "agent_id": context.agent_id,
                        "task_id": prompts.task_id(),
                        "active_session_id": active_session_id.as_str(),
                    }),
                );
                break Err(RuntimeError::NotReady("ACP session closed".to_string()));
            }
            command = command_rx.recv() => {
                let Some(command) = command else {
                    prompts.finish("ACP command channel stopped");
                    break Err(RuntimeError::NotReady("ACP command channel stopped".to_string()));
                };
                match command {
                    AcpSessionCommand::SetEventSink { sink } => {
                        attach_session_event_sink_to_slot(
                            session_event_sink,
                            pending_session_catalogs,
                            sink,
                        )?;
                    }
                    AcpSessionCommand::Prompt {
                        prompt,
                        sink,
                        done_tx,
                    } => {
                        prompts.dispatch(
                            active_session,
                            context.agent_id,
                            context.content_policy,
                            context.trace.as_ref(),
                            prompt,
                            sink,
                            done_tx,
                        );
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
                        prompts.finish("ACP session deleted");
                        break Err(RuntimeError::NotReady("ACP session deleted".to_string()));
                    }
                }
            }
            config = config_rx.recv() => {
                let Some(config) = config else {
                    prompts.finish("ACP config channel stopped");
                    break Err(RuntimeError::NotReady("ACP config channel stopped".to_string()));
                };
                handle_prompt_config_command(
                    active_session,
                    config_catalog,
                    config,
                )
                .await;
            }
            completion = prompts.next_completion() => {
                let Some(completion) = completion else {
                    prompts.finish("ACP prompt completion channel stopped");
                    break Err(RuntimeError::NotReady("ACP prompt completion channel stopped".to_string()));
                };
                let succeeded = completion.is_ok();
                logging::info(
                    "acp_prompt_result",
                    json!({
                        "agent_id": context.agent_id,
                        "task_id": prompts.task_id(),
                        "active_session_id": active_session_id.as_str(),
                        "result": if succeeded { "stop_reason" } else { "error" },
                    }),
                );
                if succeeded {
                    if let Err(error) = drain_post_prompt_updates(
                        active_session,
                        prompts.projection(),
                        context.trace.as_ref(),
                        session_event_sink.clone(),
                        pending_session_catalogs,
                    )
                    .await
                    {
                        break Err(error);
                    }
                }
                if let Some(result) = prompts.complete(completion) {
                    break result;
                }
            }
            update = active_session.read_update() => {
                let update = match update.map_err(acp_error) {
                    Ok(update) => update,
                    Err(error) => break Err(error),
                };
                match update {
                    SessionMessage::SessionMessage(dispatch) => {
                        if let Err(error) = dispatch_session_notification(
                            dispatch,
                            prompts.projection(),
                            session_event_sink.clone(),
                            pending_session_catalogs,
                        ).await {
                            break Err(error);
                        }
                    }
                    SessionMessage::StopReason(_) => {
                        logging::info(
                            "acp_prompt_update_stop_reason",
                            json!({
                                "agent_id": context.agent_id,
                                "task_id": prompts.task_id(),
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
            "task_id": prompts.task_id(),
            "active_session_id": active_session_id.as_str(),
            "result": runtime_result_name(&result),
        }),
    );
    result
}

fn runtime_result_name(result: &Result<(), RuntimeError>) -> &'static str {
    match result {
        Ok(()) => "ok",
        Err(_) => "error",
    }
}

async fn handle_prompt_config_command(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    catalog: &mut ConfigOptionsCatalog,
    command: AcpSessionConfigCommand,
) {
    match command {
        AcpSessionConfigCommand::SetConfigOption {
            agent_id,
            config_id,
            value,
            reply_tx,
        } => {
            let connection = active_session.connection();
            let result = set_task_config_option_after_prior_updates(
                &connection,
                active_session,
                config_id,
                value,
                catalog,
                &agent_id,
            )
            .await;
            if let Ok(next_catalog) = &result {
                *catalog = next_catalog.clone();
            }
            let _ = reply_tx.send(result);
        }
    }
}

async fn drain_post_prompt_updates(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    projection: LivePromptProjection,
    _trace: Option<&AcpTraceSession>,
    session_event_sink: Option<Arc<dyn AgentSessionEventSink>>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<(), RuntimeError> {
    loop {
        let update =
            tokio::time::timeout(POST_PROMPT_UPDATE_DRAIN, active_session.read_update()).await;
        let Ok(update) = update else {
            return Ok(());
        };
        match update.map_err(acp_error)? {
            SessionMessage::SessionMessage(dispatch) => {
                dispatch_session_notification(
                    dispatch,
                    projection.clone(),
                    session_event_sink.clone(),
                    pending_session_catalogs,
                )
                .await?;
            }
            SessionMessage::StopReason(_) => {}
            _ => {}
        }
    }
}

async fn dispatch_session_notification(
    dispatch: Dispatch,
    projection: LivePromptProjection,
    session_event_sink: Option<Arc<dyn AgentSessionEventSink>>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<(), RuntimeError> {
    let metadata = Arc::new(Mutex::new(None));
    let metadata_sink = metadata.clone();
    MatchDispatch::new(dispatch)
        .if_notification(async move |notification: SessionNotification| {
            *metadata_sink
                .lock()
                .expect("ACP session metadata update lock poisoned") =
                session_metadata_from_update(&notification.update);
            projection
                .emit(notification.update)
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
            Ok(())
        })
        .await
        .otherwise_ignore()
        .map_err(acp_error)?;
    let update = metadata
        .lock()
        .expect("ACP session metadata update lock poisoned")
        .take();
    if let Some(update) = update {
        deliver_session_metadata_update(
            update,
            session_event_sink.as_ref(),
            pending_session_catalogs,
        )?;
    }
    Ok(())
}
