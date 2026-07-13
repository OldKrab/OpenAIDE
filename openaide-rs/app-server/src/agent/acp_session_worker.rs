use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, Mutex};

use crate::agent::acp_schema::InitializeResponse;
use agent_client_protocol::{Agent, ConnectionTo};

use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::acp_agent_status::agent_probe_result_from_initialize;
use crate::agent::acp_host::initialize_request;
use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwnerId};
use crate::agent::acp_schema::AuthenticateRequest;
use crate::agent::acp_session_capabilities::validate_auth_method;
use crate::agent::acp_session_runner::{acp_start_error, initialize_agent_connection};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::{
    AgentAuthenticateRequest, AgentListSessionsRequest, AgentSecretResolver, AgentSession,
    AgentSessionLoad, AgentSessionStart, TurnCancellation,
};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentAuthenticateStatus, AgentListSessionsResult, AgentProbeResult,
    NormalizedMessage,
};

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_opened_session_worker::{
    run_opened_acp_session, AcpOpenedSessionWorkerInput,
};
use crate::agent::acp_session_client::{AcpSessionCommand, AcpSessionConfigCommand};
use crate::agent::acp_session_connection::{
    connect_acp_session_client, AcpSessionConnectionContext,
};
use crate::agent::acp_session_lifecycle::{
    agent_list_sessions_result_from_response, request_session_list, LoadReplayCapture,
};
use crate::agent::acp_session_opening::{open_acp_session, OpenAcpSessionContext};
use crate::agent::acp_update_projection::LivePromptProjection;

pub(super) enum AcpSessionOpenRequest {
    Start(AgentSessionStart),
    Load(AgentSessionLoad),
}

impl AcpSessionOpenRequest {
    pub(super) fn agent_id(&self) -> &str {
        match self {
            Self::Start(request) => &request.agent_id,
            Self::Load(request) => &request.agent_id,
        }
    }

    pub(super) fn task_id(&self) -> &str {
        match self {
            Self::Start(request) => &request.task_id,
            Self::Load(request) => &request.task_id,
        }
    }

    pub(super) fn operation_name(&self) -> &'static str {
        match self {
            Self::Start(_) => "session-start",
            Self::Load(_) => "session-load",
        }
    }

    pub(super) fn secret_resolver(&self) -> Option<&dyn AgentSecretResolver> {
        match self {
            Self::Start(request) => request.secret_resolver.as_deref(),
            Self::Load(request) => request.secret_resolver.as_deref(),
        }
    }

    pub(super) fn cancellation(&self) -> TurnCancellation {
        match self {
            Self::Start(request) => request.cancellation.clone(),
            Self::Load(request) => request.cancellation.clone(),
        }
    }
}

pub(super) struct AcpStartedSession {
    pub(super) session: AgentSession,
    pub(super) replayed_messages: Vec<NormalizedMessage>,
}

pub(super) struct AcpAgentProcessOpen {
    pub(super) request: AcpSessionOpenRequest,
    pub(super) command_rx: tokio_mpsc::UnboundedReceiver<AcpSessionCommand>,
    pub(super) config_rx: tokio_mpsc::UnboundedReceiver<AcpSessionConfigCommand>,
    pub(super) cancel_rx: tokio_mpsc::UnboundedReceiver<()>,
    pub(super) close_rx: tokio_mpsc::UnboundedReceiver<mpsc::Sender<Result<(), RuntimeError>>>,
    pub(super) started_tx: mpsc::Sender<Result<AcpStartedSession, String>>,
    pub(super) auth_method_id: Option<String>,
    pub(super) trace: Option<AcpTraceSession>,
    pub(super) terminal_owner_id: AcpTerminalOwnerId,
}

pub(super) struct AcpAgentProcessInput {
    pub(super) config: AcpAgentConfig,
    pub(super) first_open: Option<AcpAgentProcessOpen>,
    pub(super) open_rx: tokio_mpsc::UnboundedReceiver<AcpAgentProcessOpen>,
    pub(super) list_rx: tokio_mpsc::UnboundedReceiver<AcpAgentProcessList>,
    pub(super) control_rx: tokio_mpsc::UnboundedReceiver<AcpAgentProcessControl>,
    pub(super) shutdown_rx: tokio::sync::watch::Receiver<bool>,
    pub(super) host_bridge: HostBridge,
    pub(super) terminal_registry: AcpHostTerminalRegistry,
}

pub(super) enum AcpAgentProcessControl {
    Probe {
        agent_id: String,
        reply_tx: mpsc::Sender<Result<AgentProbeResult, RuntimeError>>,
    },
    Authenticate {
        request: AgentAuthenticateRequest,
        reply_tx: mpsc::Sender<Result<AgentAuthenticateResult, RuntimeError>>,
    },
}

pub(super) struct AcpAgentProcessList {
    pub(super) request: AgentListSessionsRequest,
    pub(super) preferred_auth_method_id: Option<String>,
    pub(super) reply_tx: mpsc::Sender<Result<AgentListSessionsResult, RuntimeError>>,
}

pub(super) async fn run_acp_agent_process(input: AcpAgentProcessInput) -> Result<(), RuntimeError> {
    let AcpAgentProcessInput {
        config,
        first_open,
        mut open_rx,
        mut list_rx,
        mut control_rx,
        mut shutdown_rx,
        host_bridge,
        terminal_registry,
    } = input;

    let current_prompts: Arc<Mutex<HashMap<String, LivePromptProjection>>> = Arc::default();
    let load_replay: Arc<Mutex<HashMap<String, LoadReplayCapture>>> = Arc::default();
    let active_session_ids: Arc<Mutex<HashSet<String>>> = Arc::default();
    let session_event_sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap =
        Arc::default();
    let session_traces: crate::agent::acp_host_capabilities::AcpSessionTraceMap = Arc::default();
    let elicitation_cancellations: crate::agent::acp_host_capabilities::AcpElicitationCancellationMap =
        Arc::default();
    let first_started_tx = first_open.as_ref().map(|open| open.started_tx.clone());
    if let Some(open) = &first_open {
        terminal_registry.begin_open(open.terminal_owner_id);
    }
    let agent = match config.to_acp_agent(
        first_open.as_ref().and_then(|open| open.trace.clone()),
        &host_bridge,
        first_open
            .as_ref()
            .and_then(|open| open.request.secret_resolver()),
    ) {
        Ok(agent) => agent,
        Err(error) => {
            if let Some(open) = &first_open {
                let _ = open.started_tx.send(Err(error.to_string()));
            }
            return Err(error);
        }
    };
    let connection_context = AcpSessionConnectionContext {
        host_bridge: host_bridge.clone(),
        trace: first_open.as_ref().and_then(|open| open.trace.clone()),
        current_prompts: current_prompts.clone(),
        load_replay: load_replay.clone(),
        terminal_registry: terminal_registry.clone(),
        session_event_sinks: session_event_sinks.clone(),
        session_traces: session_traces.clone(),
        elicitation_cancellations,
    };
    let connection_terminal_registry = terminal_registry.clone();

    let connection = connect_acp_session_client(
        agent,
        connection_context,
        |connection: ConnectionTo<Agent>| async move {
            let initialize = initialize_shared_process_connection(
                &connection,
                &host_bridge,
                first_open.as_ref(),
            )
            .await?;
            if let Some(first_open) = first_open {
                open_on_shared_process(
                    &connection,
                    initialize.clone(),
                    &host_bridge,
                    &current_prompts,
                    &load_replay,
                    &active_session_ids,
                    &connection_terminal_registry,
                    &session_event_sinks,
                    &session_traces,
                    first_open,
                )
                .await?;
            }
            loop {
                tokio::select! {
                    open = open_rx.recv() => {
                        let Some(open) = open else { break };
                        if let Err(error) = open_on_shared_process(
                            &connection,
                            initialize.clone(),
                            &host_bridge,
                            &current_prompts,
                            &load_replay,
                            &active_session_ids,
                            &connection_terminal_registry,
                            &session_event_sinks,
                            &session_traces,
                            open,
                        ).await {
                            logging::warn(
                                "acp_shared_session_open_failed",
                                serde_json::json!({ "error": error.to_string() }),
                            );
                        }
                    }
                    list = list_rx.recv() => {
                        let Some(list) = list else { break };
                        let result = list_sessions_on_shared_process(
                            &connection,
                            &initialize,
                            list.request,
                            list.preferred_auth_method_id.as_deref(),
                        ).await;
                        let _ = list.reply_tx.send(result);
                    }
                    control = control_rx.recv() => {
                        let Some(control) = control else { break };
                        match control {
                            AcpAgentProcessControl::Probe { agent_id, reply_tx } => {
                                let _ = reply_tx.send(Ok(agent_probe_result_from_initialize(agent_id, &initialize)));
                            }
                            AcpAgentProcessControl::Authenticate { request, reply_tx } => {
                                let result = authenticate_on_shared_process(&connection, &initialize, request).await;
                                let _ = reply_tx.send(result);
                            }
                        }
                    }
                }
            }
            Ok(())
        },
    );
    let result = tokio::select! {
        result = connection => result.map_err(acp_error),
        _ = shutdown_rx.changed() => Ok(()),
    };
    if let (Err(error), Some(first_started_tx)) = (&result, first_started_tx) {
        let _ = first_started_tx.send(Err(format!("ACP error: {error}")));
    }
    tokio::task::spawn_blocking(move || terminal_registry.close_all())
        .await
        .map_err(|error| RuntimeError::Internal(error.to_string()))?;
    result
}

async fn authenticate_on_shared_process(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    request: AgentAuthenticateRequest,
) -> Result<AgentAuthenticateResult, RuntimeError> {
    validate_auth_method(initialize, &request.method_id)?;
    connection
        .send_request(AuthenticateRequest::new(request.method_id.clone()))
        .block_task()
        .await
        .map_err(acp_error)?;
    Ok(AgentAuthenticateResult {
        agent_id: request.agent_id,
        method_id: request.method_id,
        status: AgentAuthenticateStatus::Authenticated,
    })
}

async fn list_sessions_on_shared_process(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    request: AgentListSessionsRequest,
    preferred_auth_method_id: Option<&str>,
) -> Result<AgentListSessionsResult, RuntimeError> {
    let cwd = std::path::PathBuf::from(&request.cwd);
    let response = request_session_list(
        connection,
        cwd.clone(),
        request.cursor,
        initialize,
        preferred_auth_method_id,
    )
    .await
    .map_err(acp_error)?;
    Ok(agent_list_sessions_result_from_response(
        request.agent_id,
        response,
        &cwd,
        None,
    ))
}

async fn initialize_shared_process_connection(
    connection: &ConnectionTo<Agent>,
    host_bridge: &HostBridge,
    first_open: Option<&AcpAgentProcessOpen>,
) -> agent_client_protocol::Result<InitializeResponse> {
    let initialize_request = initialize_request(host_bridge);
    if let Some(trace) = first_open.and_then(|open| open.trace.as_ref()) {
        trace.record("client_to_agent", "initialize.request", &initialize_request);
    }
    let Some(first_open) = first_open else {
        let initialize = connection
            .send_request(initialize_request)
            .block_task()
            .await?;
        crate::agent::acp_session_capabilities::validate_initialize_protocol(&initialize)
            .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
        return Ok(initialize);
    };
    let cancellation = first_open.request.cancellation();
    tokio::select! {
        result = initialize_agent_connection(
            connection,
            initialize_request,
            first_open.trace.as_ref(),
            &first_open.started_tx,
        ) => result,
        error = wait_for_shared_startup_cancellation(cancellation.clone()) => {
            let _ = first_open.started_tx.send(Err(error.to_string()));
            Err(acp_start_error(error))
        }
    }
}

// Session startup crosses the shared process, replay, prompt, and trace seams;
// explicit inputs make those ownership boundaries visible during orchestration.
#[allow(clippy::too_many_arguments)]
async fn open_on_shared_process(
    connection: &ConnectionTo<Agent>,
    initialize: InitializeResponse,
    host_bridge: &HostBridge,
    current_prompts: &Arc<Mutex<HashMap<String, LivePromptProjection>>>,
    load_replay: &Arc<Mutex<HashMap<String, LoadReplayCapture>>>,
    active_session_ids: &Arc<Mutex<HashSet<String>>>,
    terminal_registry: &AcpHostTerminalRegistry,
    session_event_sinks: &crate::agent::acp_host_capabilities::AcpSessionEventSinkMap,
    session_traces: &crate::agent::acp_host_capabilities::AcpSessionTraceMap,
    open: AcpAgentProcessOpen,
) -> agent_client_protocol::Result<()> {
    let AcpAgentProcessOpen {
        request,
        command_rx,
        config_rx,
        cancel_rx,
        close_rx,
        started_tx,
        auth_method_id,
        trace,
        terminal_owner_id,
    } = open;
    terminal_registry.begin_open(terminal_owner_id);
    let terminal_owner = terminal_registry.owner(terminal_owner_id);
    let request_agent_id = request.agent_id().to_string();
    let start_error_tx = started_tx.clone();
    let opened = match open_acp_session(OpenAcpSessionContext {
        connection,
        initialize: Some(initialize.clone()),
        request,
        request_agent_id: &request_agent_id,
        host_bridge,
        auth_method_id: auth_method_id.as_deref(),
        trace: trace.as_ref(),
        load_replay,
        start_error_tx: &start_error_tx,
    })
    .await
    {
        Ok(opened) => opened,
        Err(error) => {
            let owner = terminal_owner.clone();
            let _ = tokio::task::spawn_blocking(move || owner.close()).await;
            return Err(error);
        }
    };
    let started_session = opened.started_session.clone();
    let replayed_messages = opened.replayed_messages.clone();
    let session_id = started_session.session_id.clone();
    terminal_owner.activate_session(&session_id);
    let duplicate = {
        let mut active = active_session_ids
            .lock()
            .expect("ACP active session id set poisoned");
        if active.contains(&session_id) {
            true
        } else {
            active.insert(session_id.clone());
            false
        }
    };
    if duplicate {
        // The Agent session ID is already owned by another local worker. Closing the
        // rejected binding would close that shared Agent-owned session as well.
        let owner = terminal_owner.clone();
        let _ = tokio::task::spawn_blocking(move || owner.close()).await;
        let _ = started_tx.send(Err("agent_session_id already active".to_string()));
        return Ok(());
    }
    if let Some(trace) = &trace {
        // Shared Agent processes can initialize before any Task exists. Snapshot the
        // negotiated boundary into each Task trace so capability failures stay diagnosable.
        trace.record_value(
            "runtime",
            "initialize.snapshot",
            serde_json::json!({
                "sessionId": session_id,
                "source": "shared_process",
                "request": initialize_request(host_bridge),
                "response": initialize,
            }),
        );
        session_traces
            .lock()
            .expect("ACP session trace map lock poisoned")
            .insert(session_id.clone(), trace.clone());
    }
    let active_session_ids_for_task = active_session_ids.clone();
    let current_prompts_for_task = current_prompts.clone();
    let session_event_sinks_for_task = Arc::clone(session_event_sinks);
    let session_traces_for_task = Arc::clone(session_traces);
    let session_id_for_task = session_id.clone();
    tokio::spawn(async move {
        let result = run_opened_acp_session(AcpOpenedSessionWorkerInput {
            opened,
            request_agent_id,
            command_rx,
            config_rx,
            cancel_rx,
            close_rx,
            current_prompts: current_prompts_for_task,
            trace,
            session_event_sinks: session_event_sinks_for_task,
        })
        .await;
        let _ = tokio::task::spawn_blocking(move || terminal_owner.close()).await;
        active_session_ids_for_task
            .lock()
            .expect("ACP active session id set poisoned")
            .remove(&session_id);
        session_traces_for_task
            .lock()
            .expect("ACP session trace map lock poisoned")
            .remove(&session_id_for_task);
        result
    });
    let _ = started_tx.send(Ok(AcpStartedSession {
        session: started_session,
        replayed_messages,
    }));
    Ok(())
}

async fn wait_for_shared_startup_cancellation(cancellation: TurnCancellation) -> RuntimeError {
    cancellation.cancelled().await;
    RuntimeError::NotReady("ACP session start cancelled".to_string())
}
