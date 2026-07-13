use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};

use agent_client_protocol::{Agent, SessionMessage};
use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_active_prompt::send_steering_prompt_request;
use crate::agent::acp_config_options_apply::set_task_config_option_after_prior_updates;
use crate::agent::acp_prompt_runner::{
    dispatch_session_notification, run_prompt, PromptRunContext,
};
use crate::agent::acp_session_catalogs::{
    attach_session_event_sink_to_slot, PendingSessionCatalogs,
};
use crate::agent::acp_session_client::{AcpSessionCommand, AcpSessionConfigCommand};
use crate::agent::acp_session_opening::OpenedAcpSession;
use crate::agent::acp_session_termination::{close_active_session, delete_active_session};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::{AgentSession, AgentSessionEventSink};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ConfigOptionsCatalog, ConfigOptionsStatus};

pub(super) struct AcpOpenedSessionWorkerInput {
    pub(super) opened: OpenedAcpSession,
    pub(super) request_agent_id: String,
    pub(super) command_rx: tokio_mpsc::UnboundedReceiver<AcpSessionCommand>,
    pub(super) config_rx: tokio_mpsc::UnboundedReceiver<AcpSessionConfigCommand>,
    pub(super) cancel_rx: tokio_mpsc::UnboundedReceiver<()>,
    pub(super) close_rx: tokio_mpsc::UnboundedReceiver<mpsc::Sender<Result<(), RuntimeError>>>,
    pub(super) current_prompts: Arc<Mutex<HashMap<String, LivePromptProjection>>>,
    pub(super) trace: Option<AcpTraceSession>,
    pub(super) session_event_sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap,
}

pub(super) async fn run_opened_acp_session(
    input: AcpOpenedSessionWorkerInput,
) -> agent_client_protocol::Result<()> {
    let AcpOpenedSessionWorkerInput {
        opened,
        request_agent_id,
        mut command_rx,
        mut config_rx,
        mut cancel_rx,
        mut close_rx,
        current_prompts,
        trace,
        session_event_sinks,
    } = input;
    let OpenedAcpSession {
        mut active_session,
        supports_session_close,
        supports_session_delete,
        content_policy,
        started_session,
        ..
    } = opened;
    let mut session_event_sink: Option<Arc<dyn AgentSessionEventSink>> = None;
    let mut session_projection: Option<LivePromptProjection> = None;
    let mut pending_session_catalogs = PendingSessionCatalogs::default();
    let mut config_catalog = active_session_config_catalog(&started_session);
    let session_id = active_session.session_id().to_string();
    let sink_registration = SessionSinkRegistration {
        session_id,
        sinks: session_event_sinks,
    };

    loop {
        tokio::select! {
            close = close_rx.recv() => {
                let Some(reply_tx) = close else {
                    break;
                };
                let connection = active_session.connection();
                close_active_session(
                    &connection,
                    active_session.session_id().clone(),
                    supports_session_close,
                    trace.as_ref(),
                )
                .await;
                let _ = reply_tx.send(Ok(()));
                break;
            }
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                match command {
                    AcpSessionCommand::SetEventSink { sink } => {
                        sink_registration.set(sink.clone());
                        attach_session_event_sink_to_slot(
                            &mut session_event_sink,
                            &mut pending_session_catalogs,
                            sink.clone(),
                        )
                        .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
                        session_projection = Some(LivePromptProjection::for_session(
                            request_agent_id.clone(),
                            sink,
                        ));
                    }
                    AcpSessionCommand::Prompt {
                        prompt,
                        sink,
                        done_tx,
                    } => {
                        let result = run_prompt(
                            &mut active_session,
                            &mut cancel_rx,
                            &mut close_rx,
                            PromptRunContext {
                                agent_id: &request_agent_id,
                                supports_session_close,
                                supports_session_delete,
                                current_prompts: &current_prompts,
                                trace: trace.clone(),
                                content_policy,
                            },
                            prompt,
                            sink,
                            &mut command_rx,
                            &mut config_rx,
                            &mut config_catalog,
                            &mut session_event_sink,
                            &mut session_projection,
                            &mut pending_session_catalogs,
                        )
                        .await;
                        let _ = done_tx.send(result);
                    }
                    AcpSessionCommand::Steer { prompt } => {
                        if let Err(error) = send_steering_prompt_request(
                            &active_session,
                            prompt,
                            content_policy,
                            trace.as_ref(),
                        ) {
                            crate::logging::error(
                                "acp_steering_prompt_start_failed",
                                serde_json::json!({ "error": error.to_string() }),
                            );
                        }
                    }
                    AcpSessionCommand::Delete { reply_tx } => {
                        let connection = active_session.connection();
                        let result = delete_active_session(
                            &connection,
                            active_session.session_id().clone(),
                            supports_session_delete,
                            trace.as_ref(),
                        )
                        .await;
                        let _ = reply_tx.send(result);
                        break;
                    }
                }
            }
            config = config_rx.recv() => {
                let Some(config) = config else {
                    break;
                };
                handle_session_config_command(
                    &mut active_session,
                    &mut config_catalog,
                    session_event_sink.as_ref(),
                    session_projection.clone(),
                    &mut pending_session_catalogs,
                    config,
                )
                .await
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
            }
            update = active_session.read_update() => {
                let update = update?;
                apply_opened_session_message(
                    &request_agent_id,
                    update,
                    &mut config_catalog,
                    session_event_sink.as_ref(),
                    session_projection.clone(),
                    &mut pending_session_catalogs,
                )
                .await
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
            }
        }
    }

    Ok(())
}

struct SessionSinkRegistration {
    session_id: String,
    sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap,
}

impl SessionSinkRegistration {
    fn set(&self, sink: Arc<dyn AgentSessionEventSink>) {
        self.sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .insert(self.session_id.clone(), sink);
    }
}

impl Drop for SessionSinkRegistration {
    fn drop(&mut self) {
        self.sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .remove(&self.session_id);
    }
}

fn active_session_config_catalog(session: &AgentSession) -> ConfigOptionsCatalog {
    session
        .config_catalog
        .clone()
        .unwrap_or_else(|| ConfigOptionsCatalog {
            agent_id: String::new(),
            status: ConfigOptionsStatus::Empty,
            options: Vec::new(),
        })
}

async fn apply_opened_session_message(
    agent_id: &str,
    update: SessionMessage,
    config_catalog: &mut ConfigOptionsCatalog,
    session_event_sink: Option<&Arc<dyn AgentSessionEventSink>>,
    session_projection: Option<LivePromptProjection>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<(), RuntimeError> {
    let SessionMessage::SessionMessage(dispatch) = update else {
        return Ok(());
    };
    if let Some(catalog) = dispatch_session_notification(
        agent_id,
        dispatch,
        session_projection,
        session_event_sink.cloned(),
        pending_session_catalogs,
    )
    .await?
    {
        *config_catalog = catalog;
    }
    Ok(())
}

async fn handle_session_config_command(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    catalog: &mut ConfigOptionsCatalog,
    session_event_sink: Option<&Arc<dyn AgentSessionEventSink>>,
    session_projection: Option<LivePromptProjection>,
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
                if let Err(error) = apply_opened_session_message(
                    &agent_id,
                    update,
                    catalog,
                    session_event_sink,
                    session_projection.clone(),
                    pending_session_catalogs,
                )
                .await
                {
                    let _ = reply_tx.send(Err(RuntimeError::Internal(error.to_string())));
                    return Err(error);
                }
            }
            let result =
                response.finish_with_session_sink(session_event_sink.map(|sink| sink.as_ref()));
            if let Ok(next_catalog) = &result {
                *catalog = next_catalog.clone();
            }
            let _ = reply_tx.send(result);
        }
    }
    Ok(())
}
