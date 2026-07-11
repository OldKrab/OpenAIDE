use std::path::PathBuf;
use std::sync::mpsc;

use agent_client_protocol::schema::{
    InitializeRequest, ProtocolVersion, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionNotification,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Handled, SessionMessage};
use tokio::sync::mpsc as tokio_mpsc;

use crate::protocol::errors::RuntimeError;

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::acp_config_options_apply::{
    set_prepared_config_option_after_prior_updates, PreparedOptionsSetContext,
};
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_options_session_client::{AcpOptionsCommand, AcpOptionsCommandReceiver};
use crate::agent::acp_session_runner::{initialize_agent_connection, AcpSessionRunner};
use crate::agent::acp_update_projection::PreparedOptionsProjection;

pub(super) struct AcpOptionsSessionWorkerInput {
    pub(super) config: AcpAgentConfig,
    pub(super) agent_id: String,
    pub(super) cwd: PathBuf,
    pub(super) auth_method_id: Option<String>,
    pub(super) command_rx: AcpOptionsCommandReceiver,
    pub(super) started_tx: mpsc::Sender<Result<(), String>>,
    pub(super) host_bridge: crate::protocol::host::HostBridge,
}

pub(super) async fn run_options_session(
    input: AcpOptionsSessionWorkerInput,
) -> Result<(), RuntimeError> {
    let AcpOptionsSessionWorkerInput {
        config,
        agent_id,
        cwd,
        auth_method_id,
        command_rx,
        started_tx,
        host_bridge,
    } = input;
    let mut command_rx = command_rx.0;
    let agent = match config.to_acp_agent(None, &host_bridge, None) {
        Ok(agent) => agent,
        Err(error) => {
            let _ = started_tx.send(Err(error.to_string()));
            return Err(error);
        }
    };
    let start_error_tx = started_tx.clone();
    let (invalidation_tx, mut invalidation_rx) = tokio_mpsc::unbounded_channel::<String>();
    let permission_invalidation_tx = invalidation_tx.clone();

    Client
        .builder()
        .name("openaide")
        .on_receive_notification(
            async move |notification: SessionNotification, cx| {
                Ok(Handled::No {
                    message: (notification, cx),
                    retry: false,
                })
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |_request: RequestPermissionRequest, responder, _connection| {
                let _ = permission_invalidation_tx
                    .send("ACP options session received task activity".to_string());
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
            let initialize = initialize_agent_connection(
                &connection,
                InitializeRequest::new(ProtocolVersion::V1),
                None,
                &start_error_tx,
            )
            .await?;
            let runner = AcpSessionRunner::new(
                &agent_id,
                &connection,
                initialize,
                auth_method_id.as_deref(),
                None,
            );

            let (mut active_session, initial_options) =
                match runner.start(cwd).await {
                    Ok(session) => session,
                    Err(error) => {
                        let _ = start_error_tx.send(Err(format!("ACP error: {error}")));
                        return Err(error);
                    }
                };
            let options_projection = PreparedOptionsProjection::new(&agent_id);
            let mut catalog = options_projection.catalog(initial_options);
            let _ = started_tx.send(Ok(()));

            loop {
                tokio::select! {
                    invalidation = invalidation_rx.recv() => {
                        return Err(agent_client_protocol::util::internal_error(
                            invalidation.unwrap_or_else(|| "ACP options session invalidated".to_string()),
                        ));
                    }
                    command = command_rx.recv() => {
                        let Some(command) = command else {
                            break;
                        };
                        match command {
                            AcpOptionsCommand::Get { reply_tx } => {
                                let _ = reply_tx.send(Ok(catalog.clone()));
                            }
                            AcpOptionsCommand::Set { config_id, value, reply_tx } => {
                                let result = set_prepared_config_option_after_prior_updates(
                                    &connection,
                                    &mut active_session,
                                    config_id,
                                    value,
                                    &mut catalog,
                                    PreparedOptionsSetContext {
                                        invalidation_rx: &mut invalidation_rx,
                                        agent_id: &agent_id,
                                    },
                                )
                                .await;
                                if let Ok(next_catalog) = &result {
                                    catalog = next_catalog.clone();
                                }
                                let invalidated = matches!(result, Err(RuntimeError::NotReady(_)));
                                let _ = reply_tx.send(result);
                                if invalidated {
                                    return Err(agent_client_protocol::util::internal_error(
                                        "ACP options session received task activity",
                                    ));
                                }
                            }
                            AcpOptionsCommand::List { agent_id, cwd, cursor, reply_tx } => {
                                let result = runner.list_sessions(
                                    &active_session,
                                    agent_id,
                                    cwd,
                                    cursor,
                                )
                                .await;
                                let _ = reply_tx.send(result);
                            }
                            AcpOptionsCommand::Close { reply_tx } => {
                                runner.close(active_session.session_id().clone()).await;
                                let _ = reply_tx.send(Ok(()));
                                break;
                            }
                        }
                    }
                    update = active_session.read_update() => {
                        match update {
                            Ok(SessionMessage::SessionMessage(dispatch)) => {
                                options_projection.apply_dispatch(dispatch, &mut catalog)
                                .await
                                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
                            }
                            Ok(SessionMessage::StopReason(_)) => {}
                            Ok(_) => {}
                            Err(error) => return Err(error),
                        }
                    }
                }
            }

            Ok(())
        })
        .await
        .map_err(acp_error)
}
