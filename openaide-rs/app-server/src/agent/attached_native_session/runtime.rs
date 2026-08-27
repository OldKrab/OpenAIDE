use std::sync::Arc;
use std::time::Duration;

use crate::agent::acp_active_prompt::{cancel_active_prompt, send_steering_prompt_request};
use crate::agent::acp_config_options_apply::set_task_config_option_after_prior_updates;
use crate::agent::acp_prompt_runner::{
    dispatch_session_notification, run_prompt, PromptRunContext,
};
use crate::agent::acp_session_catalogs::{
    attach_session_event_sink_with_catalog_snapshot, session_with_catalog_snapshots,
    PendingSessionCatalogs,
};
use crate::agent::acp_session_opening::OpenedAcpSession;
use crate::agent::acp_session_paths::normalized_session_cwd;
use crate::agent::acp_session_runner::AcpSessionRunner;
use crate::agent::acp_session_termination::{close_active_session, delete_active_session};
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::{
    AgentLoadedSession, AgentPromptCapabilities, AgentSession, AgentSessionEventSink,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentCommandsCatalog, ConfigOptionsCatalog, ConfigOptionsStatus};
use agent_client_protocol::{Agent, SessionMessage};

const IDLE_SESSION_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

use super::{AcpSessionCommand, AcpSessionConfigCommand, AttachedNativeSessionRunInput};

pub(super) async fn run(
    runtime: AttachedNativeSessionRunInput,
) -> agent_client_protocol::Result<()> {
    let AttachedNativeSessionRunInput {
        opened,
        request_agent_id,
        initialize,
        auth_method_id,
        load_replay,
        mut command_rx,
        mut config_rx,
        mut cancel_rx,
        mut close_rx,
        current_prompts,
        trace,
        session_event_sinks,
        session_idle_timeout,
    } = runtime;
    let OpenedAcpSession {
        mut active_session,
        supports_session_close,
        supports_session_delete,
        mut idle_close_eligible,
        content_policy,
        started_session,
        ..
    } = opened;
    // The attachment owns the current session snapshot. The registry can ask for
    // this state when a caller reuses an existing attachment instead of
    // issuing a duplicate ACP session/resume request.
    let mut session_snapshot = started_session.clone();
    let mut session_event_sink: Option<Arc<dyn AgentSessionEventSink>> = None;
    let mut session_projection: Option<LivePromptProjection> = None;
    let mut pending_session_catalogs = PendingSessionCatalogs::default();
    let mut config_catalog = active_session_config_catalog(&started_session);
    let mut commands_catalog = started_session.commands_catalog.clone();
    let session_id = active_session.session_id().to_string();
    let sink_registration = SessionSinkRegistration {
        session_id,
        sinks: session_event_sinks,
    };
    let idle_deadline = tokio::time::sleep(session_idle_timeout);
    tokio::pin!(idle_deadline);

    loop {
        tokio::select! {
            biased;
            close = close_rx.recv() => {
                let Some(reply_tx) = close else {
                    break;
                };
                let connection = active_session.connection();
                close_active_session(
                    connection,
                    active_session.session_id().clone(),
                    supports_session_close,
                    trace.as_ref(),
                )
                .await;
                let _ = reply_tx.send(Ok(()));
                break;
            }
            Some(()) = cancel_rx.recv() => {
                if let Err(error) = cancel_active_prompt(&active_session, trace.as_ref()).await {
                    crate::logging::error(
                        "acp_idle_prompt_cancel_failed",
                        serde_json::json!({ "error": error.to_string() }),
                    );
                }
            }
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                match command {
                    AcpSessionCommand::Snapshot { reply_tx } => {
                        let _ = reply_tx.send(Ok(session_snapshot.clone()));
                    }
                    AcpSessionCommand::SetEventSink { sink } => {
                        sink_registration.set(sink.clone());
                        attach_session_event_sink_with_catalog_snapshot(
                            &mut session_event_sink,
                            &mut pending_session_catalogs,
                            session_snapshot.config_catalog.as_ref().filter(|catalog| {
                                catalog.status != ConfigOptionsStatus::Empty
                            }),
                            session_snapshot.commands_catalog.as_ref(),
                            sink.clone(),
                        )
                        .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
                        session_projection = Some(LivePromptProjection::for_session(
                            request_agent_id.clone(),
                            sink,
                        ));
                    }
                    AcpSessionCommand::Load { request, reply_tx } => {
                        let mcp_servers = match request.secret_resolver.as_deref() {
                            Some(resolver) => resolver.resolve_mcp_servers(
                                &initialize.agent_capabilities.mcp_capabilities,
                            ),
                            None => Ok(Vec::new()),
                        };
                        let mcp_servers = match mcp_servers {
                            Ok(servers) => servers,
                            Err(error) => {
                                let _ = reply_tx.send(Err(error));
                                continue;
                            }
                        };
                        let connection = active_session.connection();
                        let runner = AcpSessionRunner::new(
                            &request_agent_id,
                            connection,
                            initialize.clone(),
                            auth_method_id.as_deref(),
                            trace.as_ref(),
                        );
                        let result = runner
                            .load(
                                request.session_id,
                                normalized_session_cwd(&request.cwd),
                                mcp_servers,
                                &load_replay,
                            )
                            .await
                            .map(|(reloaded, catalog, commands, replay)| {
                                let session_id = reloaded.session_id().to_string();
                                active_session = reloaded;
                                config_catalog = catalog.clone();
                                let session = AgentSession::new(
                                    request_agent_id.clone(),
                                    session_id,
                                )
                                .with_commands_catalog(commands)
                                .with_replayed_plan(replay.plan)
                                .with_prompt_capabilities(AgentPromptCapabilities {
                                    image: content_policy.capabilities.image,
                                })
                                .with_config_options(&catalog);
                                AgentLoadedSession {
                                    session,
                                    replayed_messages: replay.messages,
                                }
                            });
                        if let Ok(loaded) = &result {
                            session_snapshot = loaded.session.clone();
                            commands_catalog = loaded.session.commands_catalog.clone();
                        }
                        if result.is_ok() {
                            idle_close_eligible = true;
                        }
                        let _ = reply_tx.send(result);
                    }
                    AcpSessionCommand::Prompt {
                        prompt,
                        sink,
                        done_tx,
                        request_guard,
                    } => {
                        crate::logging::info(
                            "acp_prompt_command_received",
                            serde_json::json!({
                                "task_id": prompt.task_id.as_str(),
                                "session_id": prompt.session_id.as_str(),
                            }),
                        );
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
                            request_guard,
                            &mut command_rx,
                            &mut config_rx,
                            &mut config_catalog,
                            &mut commands_catalog,
                            &session_snapshot,
                            &mut session_event_sink,
                            &mut session_projection,
                            &mut pending_session_catalogs,
                        )
                        .await;
                        session_snapshot = session_with_catalog_snapshots(
                            &session_snapshot,
                            &config_catalog,
                            &commands_catalog,
                        );
                        if result.is_ok() {
                            idle_close_eligible = true;
                        }
                        let _ = done_tx.send(result);
                    }
                    AcpSessionCommand::Steer {
                        prompt,
                        request_guard,
                    } => {
                        if let Err(error) = send_steering_prompt_request(
                            &active_session,
                            prompt,
                            content_policy,
                            trace.as_ref(),
                            None,
                            None,
                            request_guard,
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
                            connection,
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
                    &mut commands_catalog,
                    session_event_sink.as_ref(),
                    session_projection.clone(),
                    &mut pending_session_catalogs,
                    config,
                )
                .await
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
                session_snapshot = session_with_catalog_snapshots(
                    &session_snapshot,
                    &config_catalog,
                    &commands_catalog,
                );
            }
            update = active_session.read_update() => {
                let update = update?;
                apply_opened_session_message(
                    &request_agent_id,
                    update,
                    &mut config_catalog,
                    &mut commands_catalog,
                    session_event_sink.as_ref(),
                    session_projection.clone(),
                    &mut pending_session_catalogs,
                )
                .await
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
                session_snapshot = session_with_catalog_snapshots(
                    &session_snapshot,
                    &config_catalog,
                    &commands_catalog,
                );
            }
            () = &mut idle_deadline, if supports_session_close && idle_close_eligible => {
                let idle_session_id = active_session.session_id().clone();
                crate::logging::info(
                    "acp_session_idle_timeout",
                    serde_json::json!({
                        "agent_id": request_agent_id,
                        "session_id": idle_session_id.to_string(),
                        "idle_timeout_ms": session_idle_timeout.as_millis(),
                    }),
                );
                let connection = active_session.connection();
                if tokio::time::timeout(
                    IDLE_SESSION_CLOSE_TIMEOUT,
                    close_active_session(
                        connection,
                        idle_session_id.clone(),
                        supports_session_close,
                        trace.as_ref(),
                    ),
                )
                .await
                .is_err()
                {
                    crate::logging::warn(
                        "acp_session_idle_close_timed_out",
                        serde_json::json!({
                            "agent_id": request_agent_id,
                            "session_id": idle_session_id.to_string(),
                            "close_timeout_ms": IDLE_SESSION_CLOSE_TIMEOUT.as_millis(),
                        }),
                    );
                }
                break;
            }
        }
        idle_deadline
            .as_mut()
            .reset(tokio::time::Instant::now() + session_idle_timeout);
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
    commands_catalog: &mut Option<AgentCommandsCatalog>,
    session_event_sink: Option<&Arc<dyn AgentSessionEventSink>>,
    session_projection: Option<LivePromptProjection>,
    pending_session_catalogs: &mut PendingSessionCatalogs,
) -> Result<(), RuntimeError> {
    let SessionMessage::SessionMessage(dispatch) = update else {
        return Ok(());
    };
    let catalogs = dispatch_session_notification(
        agent_id,
        dispatch,
        session_projection,
        session_event_sink.cloned(),
        pending_session_catalogs,
    )
    .await?;
    apply_session_catalogs(catalogs, config_catalog, commands_catalog);
    Ok(())
}

async fn handle_session_config_command(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    catalog: &mut ConfigOptionsCatalog,
    commands_catalog: &mut Option<AgentCommandsCatalog>,
    session_event_sink: Option<&Arc<dyn AgentSessionEventSink>>,
    session_projection: Option<LivePromptProjection>,
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
            crate::logging::info(
                "acp_config_option_command_received",
                serde_json::json!({
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
                if let Err(error) = apply_opened_session_message(
                    &agent_id,
                    update,
                    catalog,
                    commands_catalog,
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
            crate::logging::info(
                "acp_config_option_catalog_published",
                serde_json::json!({
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

fn apply_session_catalogs(
    catalogs: crate::agent::acp_session_catalogs::DispatchSessionCatalogs,
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
