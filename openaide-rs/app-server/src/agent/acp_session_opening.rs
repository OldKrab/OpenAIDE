use std::sync::mpsc;

use crate::agent::acp_schema::InitializeResponse;
use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_config_options_apply::apply_config_options;
use crate::agent::acp_host::initialize_request;
use crate::agent::acp_session_lifecycle::LoadReplayCaptures;
use crate::agent::acp_session_paths::normalized_session_cwd;
use crate::agent::acp_session_runner::{
    acp_start_error, initialize_agent_connection, AcpActiveSession, AcpSessionRunner,
};
use crate::agent::acp_session_worker::{AcpSessionOpenRequest, AcpStartedSession};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::prompt_content::{
    validate_prompt_attachments, PromptContentCapabilities, PromptContentPolicy,
};
use crate::agent::AgentSession;
use crate::agent::TurnCancellation;
use crate::protocol::host::HostBridge;
use crate::protocol::model::NormalizedMessage;

pub(super) struct OpenAcpSessionContext<'a> {
    pub(super) connection: &'a ConnectionTo<Agent>,
    pub(super) initialize: Option<InitializeResponse>,
    pub(super) request: AcpSessionOpenRequest,
    pub(super) request_agent_id: &'a str,
    pub(super) host_bridge: &'a HostBridge,
    pub(super) auth_method_id: Option<&'a str>,
    pub(super) trace: Option<&'a AcpTraceSession>,
    pub(super) load_replay: &'a LoadReplayCaptures,
    pub(super) start_error_tx:
        &'a mpsc::Sender<Result<AcpStartedSession, crate::protocol::errors::RuntimeError>>,
}

pub(super) struct OpenedAcpSession {
    pub(super) active_session: AcpActiveSession,
    pub(super) supports_session_close: bool,
    pub(super) supports_session_delete: bool,
    pub(super) content_policy: PromptContentPolicy,
    pub(super) started_session: AgentSession,
    pub(super) replayed_messages: Vec<NormalizedMessage>,
}

pub(super) async fn open_acp_session<'a>(
    context: OpenAcpSessionContext<'a>,
) -> Result<OpenedAcpSession, agent_client_protocol::Error> {
    let cancellation = context.request.cancellation();
    let initialize = match context.initialize {
        Some(initialize) => initialize,
        None => {
            let initialize_request = initialize_request(context.host_bridge);
            if let Some(trace) = context.trace {
                trace.record("client_to_agent", "initialize.request", &initialize_request);
            }
            tokio::select! {
                result = initialize_agent_connection(
                    context.connection,
                    initialize_request,
                    context.trace,
                    context.start_error_tx,
                ) => result?,
                error = wait_for_startup_cancellation(cancellation.clone()) => {
                    let _ = context.start_error_tx.send(Err(error.clone()));
                    return Err(acp_start_error(error));
                }
            }
        }
    };
    let runner = AcpSessionRunner::new(
        context.request_agent_id,
        context.connection,
        initialize,
        context.auth_method_id,
        context.trace,
    );
    let supports_session_close = runner.supports_session_close();
    let content_policy = prompt_content_policy(runner.initialize());

    if let AcpSessionOpenRequest::Start(request) = &context.request {
        if let Err(error) = validate_prompt_attachments(&request.context, content_policy) {
            let _ = context.start_error_tx.send(Err(
                crate::protocol::errors::RuntimeError::InvalidParams(error.to_string()),
            ));
            return Err(agent_client_protocol::util::internal_error(
                error.to_string(),
            ));
        }
    }

    let (active_session, applied_options, replayed_commands, replayed_messages) = match context
        .request
    {
        AcpSessionOpenRequest::Start(request) => {
            let session_cwd = normalized_session_cwd(&request.cwd);
            let start_result = tokio::select! {
                result = runner.start(session_cwd) => result,
                error = wait_for_startup_cancellation(request.cancellation.clone()) => {
                    let _ = context.start_error_tx.send(Err(error.clone()));
                    return Err(acp_start_error(error));
                }
            };
            let (mut active_session, initial_options) = match start_result {
                Ok(session) => session,
                Err(error) => {
                    let _ = context
                        .start_error_tx
                        .send(Err(crate::agent::acp_errors::acp_error(&error)));
                    return Err(error);
                }
            };
            let applied_options = match apply_config_options(
                context.request_agent_id,
                context.connection,
                &mut active_session,
                initial_options,
                request.config_options.as_ref(),
                request.config_option_policy,
            )
            .await
            {
                Ok(catalog) => catalog,
                Err(error) => {
                    runner.close(active_session.session_id().clone()).await;
                    let _ = context.start_error_tx.send(Err(error.clone()));
                    return Err(acp_start_error(error));
                }
            };
            (active_session, Some(applied_options), None, Vec::new())
        }
        AcpSessionOpenRequest::Load(request) => {
            let session_cwd = normalized_session_cwd(&request.cwd);
            let load_result = tokio::select! {
                result = runner.load(request.session_id, session_cwd, context.load_replay) => result,
                error = wait_for_startup_cancellation(request.cancellation.clone()) => Err(error),
            };
            match load_result {
                Ok((active_session, catalog, commands, messages)) => {
                    (active_session, Some(catalog), commands, messages)
                }
                Err(error) => {
                    let _ = context.start_error_tx.send(Err(error.clone()));
                    return Err(acp_start_error(error));
                }
            }
        }
        AcpSessionOpenRequest::Resume(request) => {
            let session_cwd = normalized_session_cwd(&request.cwd);
            let resume_result = tokio::select! {
                result = runner.resume(request.session_id, session_cwd) => result,
                error = wait_for_startup_cancellation(request.cancellation.clone()) => Err(error),
            };
            match resume_result {
                Ok((active_session, catalog)) => (active_session, catalog, None, Vec::new()),
                Err(error) => {
                    let _ = context.start_error_tx.send(Err(error.clone()));
                    return Err(acp_start_error(error));
                }
            }
        }
    };

    let session_id = active_session.session_id().to_string();
    let mut started_session = AgentSession::new(context.request_agent_id, session_id)
        .with_commands_catalog(replayed_commands)
        .with_prompt_capabilities(crate::agent::AgentPromptCapabilities {
            image: content_policy.capabilities.image,
        });
    if let Some(applied_options) = applied_options {
        started_session = started_session.with_config_options(&applied_options);
    }
    Ok(OpenedAcpSession {
        active_session,
        supports_session_close,
        supports_session_delete: runner.supports_session_delete(),
        content_policy,
        started_session,
        replayed_messages,
    })
}

async fn wait_for_startup_cancellation(
    cancellation: TurnCancellation,
) -> crate::protocol::errors::RuntimeError {
    cancellation.cancelled().await;
    crate::protocol::errors::RuntimeError::NotReady("ACP session start cancelled".to_string())
}

fn prompt_content_policy(initialize: &InitializeResponse) -> PromptContentPolicy {
    let capabilities = &initialize.agent_capabilities.prompt_capabilities;
    PromptContentPolicy::new(PromptContentCapabilities {
        image: capabilities.image,
        audio: capabilities.audio,
        embedded_context: capabilities.embedded_context,
    })
}
