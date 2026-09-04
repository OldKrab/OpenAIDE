use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use crate::agent::acp_schema::InitializeResponse;
use agent_client_protocol::{Agent, ConnectionTo};

use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::acp_agent_status::agent_probe_result_from_initialize;
use crate::agent::acp_host::initialize_request;
use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwnerId};
use crate::agent::acp_process_diagnostics::acp_connection_terminal_diagnostics;
use crate::agent::acp_schema::{CloseSessionRequest, ForkSessionRequest};
use crate::agent::acp_session_capabilities::validate_session_fork_capabilities;
use crate::agent::acp_session_runner::{acp_start_error, initialize_agent_connection};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::{
    AgentAuthenticateRequest, AgentForkedSession, AgentListSessionsRequest, AgentSecretResolver,
    AgentSession, AgentSessionFork, AgentSessionLoad, AgentSessionResume, AgentSessionStart,
    TurnCancellation,
};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult, NormalizedMessage,
};

use crate::agent::acp_agent_authentication::authenticate_on_shared_process;
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_session_connection::{
    connect_acp_session_client, AcpSessionConnectionContext,
};
use crate::agent::acp_session_lifecycle::{
    agent_list_sessions_result_from_response, request_session_list, LoadReplayCapture,
};
use crate::agent::acp_session_opening::{open_acp_session, OpenAcpSessionContext};
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::attached_native_session::{
    AcpSessionCommand, AcpSessionConfigCommand, AttachedNativeSession,
    AttachedNativeSessionRunInput,
};

#[derive(Clone)]
pub(super) enum AcpSessionOpenRequest {
    Start(AgentSessionStart),
    Load(AgentSessionLoad),
    Resume(AgentSessionResume),
}

impl AcpSessionOpenRequest {
    pub(super) fn agent_id(&self) -> &str {
        match self {
            Self::Start(request) => &request.agent_id,
            Self::Load(request) => &request.agent_id,
            Self::Resume(request) => &request.agent_id,
        }
    }

    pub(super) fn task_id(&self) -> &str {
        match self {
            Self::Start(request) => &request.task_id,
            Self::Load(request) => &request.task_id,
            Self::Resume(request) => &request.task_id,
        }
    }

    pub(super) fn operation_name(&self) -> &'static str {
        match self {
            Self::Start(_) => "session-start",
            Self::Load(_) => "session-load",
            Self::Resume(_) => "session-resume",
        }
    }

    pub(super) fn secret_resolver(&self) -> Option<&dyn AgentSecretResolver> {
        match self {
            Self::Start(request) => request.secret_resolver.as_deref(),
            Self::Load(request) => request.secret_resolver.as_deref(),
            Self::Resume(request) => request.secret_resolver.as_deref(),
        }
    }

    pub(super) fn cancellation(&self) -> TurnCancellation {
        match self {
            Self::Start(request) => request.cancellation.clone(),
            Self::Load(request) => request.cancellation.clone(),
            Self::Resume(request) => request.cancellation.clone(),
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
    pub(super) started_tx: mpsc::Sender<Result<AcpStartedSession, RuntimeError>>,
    pub(super) auth_method_id: Option<String>,
    pub(super) trace: Option<AcpTraceSession>,
    pub(super) terminal_owner_id: AcpTerminalOwnerId,
    /// Releases inactive Agent resources without coupling lifetime to Task pages.
    pub(super) session_idle_timeout: std::time::Duration,
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
    pub(super) secret_resolver: Option<Arc<dyn AgentSecretResolver>>,
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
    Logout {
        agent_id: String,
        reply_tx: mpsc::Sender<Result<(), RuntimeError>>,
    },
    Fork {
        request: AgentSessionFork,
        reply_tx: mpsc::Sender<Result<AgentForkedSession, RuntimeError>>,
    },
}

pub(super) struct AcpAgentProcessList {
    pub(super) request: AgentListSessionsRequest,
    pub(super) preferred_auth_method_id: Option<String>,
    pub(super) timeout: std::time::Duration,
    pub(super) reply_tx: mpsc::Sender<Result<AgentListSessionsResult, RuntimeError>>,
}

/// Owns one shared ACP Agent process and dispatches its Native Session attachments.
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
        secret_resolver,
    } = input;

    let current_prompts: Arc<Mutex<HashMap<String, LivePromptProjection>>> = Arc::default();
    let load_replay: Arc<Mutex<HashMap<String, LoadReplayCapture>>> = Arc::default();
    let active_session_ids: Arc<Mutex<HashSet<String>>> = Arc::default();
    let session_event_sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap =
        Arc::default();
    let native_subagents = crate::agent::acp_native_subagents::AcpNativeSubagentRouter::new(
        config.agent_id.clone(),
        session_event_sinks.clone(),
    );
    let session_traces: crate::agent::acp_host_capabilities::AcpSessionTraceMap = Arc::default();
    let diagnostic_current_prompts = current_prompts.clone();
    let diagnostic_active_session_ids = active_session_ids.clone();
    let elicitation_cancellations: crate::agent::acp_host_capabilities::AcpElicitationCancellationMap =
        Arc::default();
    let first_started_tx = first_open.as_ref().map(|open| open.started_tx.clone());
    if let Some(open) = &first_open {
        terminal_registry.begin_open(open.terminal_owner_id);
    }
    let agent_id = config.agent_id.clone();
    let initial_operation = first_open
        .as_ref()
        .map(|open| open.request.operation_name().to_string());
    let initial_task_id = first_open
        .as_ref()
        .map(|open| open.request.task_id().to_string());
    let has_initial_session = first_open.is_some();
    let launcher_kind = config.diagnostic_launcher_kind();
    logging::info(
        "acp_agent_launch_selected",
        serde_json::json!({
            "agent_id": agent_id,
            "has_initial_session": has_initial_session,
            "launcher_kind": launcher_kind,
            "operation": initial_operation,
            "task_id": initial_task_id,
        }),
    );
    let agent = match config.to_acp_agent(
        first_open.as_ref().and_then(|open| open.trace.clone()),
        &host_bridge,
        first_open
            .as_ref()
            .and_then(|open| open.request.secret_resolver())
            .or(secret_resolver.as_deref()),
    ) {
        Ok(agent) => agent,
        Err(error) => {
            if let Some(open) = &first_open {
                let _ = open.started_tx.send(Err(error.clone()));
            }
            return Err(error);
        }
    };
    let connection_context = AcpSessionConnectionContext {
        agent_id: config.agent_id.clone(),
        host_bridge: host_bridge.clone(),
        trace: first_open.as_ref().and_then(|open| open.trace.clone()),
        current_prompts: current_prompts.clone(),
        load_replay: load_replay.clone(),
        terminal_registry: terminal_registry.clone(),
        session_event_sinks: session_event_sinks.clone(),
        session_traces: session_traces.clone(),
        elicitation_cancellations,
        native_subagents: native_subagents.clone(),
    };
    let connection_terminal_registry = terminal_registry.clone();

    let connection_started_at = Instant::now();
    logging::info(
        "acp_agent_connection_started",
        serde_json::json!({
            "agent_id": agent_id,
            "has_initial_session": has_initial_session,
            "launcher_kind": launcher_kind,
            "operation": initial_operation,
            "task_id": initial_task_id,
        }),
    );
    let connection = connect_acp_session_client(
        agent,
        connection_context,
        |connection: ConnectionTo<Agent>| async move {
            let initialize = initialize_shared_process_connection(
                &connection,
                &host_bridge,
                &config.agent_id,
                first_open.as_ref(),
            )
            .await?;
            let client_enabled = crate::agent::acp_host::native_subagents_enabled();
            let agent_advertised = initialize
                .agent_capabilities
                .session_capabilities
                .subagents
                .is_some();
            let negotiated = client_enabled && agent_advertised;
            native_subagents.set_negotiated(negotiated);
            crate::logging::info(
                "acp_native_subagents_negotiated",
                serde_json::json!({
                    "agent_id": config.agent_id,
                    "client_enabled": client_enabled,
                    "agent_advertised": agent_advertised,
                    "negotiated": negotiated,
                }),
            );
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
                        let operation = open.request.operation_name();
                        let task_id = open.request.task_id().to_string();
                        let started_at = Instant::now();
                        logging::info(
                            "acp_shared_session_open_started",
                            serde_json::json!({
                                "operation": operation,
                                "task_id": task_id,
                                "agent_id": open.request.agent_id(),
                            }),
                        );
                        if open_on_shared_process(
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
                        )
                        .await
                        .is_err()
                        {
                            logging::warn(
                                "acp_shared_session_open_failed",
                                serde_json::json!({
                                    "operation": operation,
                                    "task_id": task_id,
                                    "duration_ms": started_at.elapsed().as_millis(),
                                    "error_kind": "acp_error",
                                }),
                            );
                        } else {
                            logging::info(
                                "acp_shared_session_open_completed",
                                serde_json::json!({
                                    "operation": operation,
                                    "task_id": task_id,
                                    "duration_ms": started_at.elapsed().as_millis(),
                                }),
                            );
                        }
                    }
                    list = list_rx.recv() => {
                        let Some(list) = list else { break };
                        // Discovery is best-effort background work. Bound only its ACP request;
                        // active Native Session attachments continue on the shared connection.
                        let result = tokio::time::timeout(
                            list.timeout,
                            list_sessions_on_shared_process(
                                &connection,
                                &initialize,
                                list.request,
                                list.preferred_auth_method_id.as_deref(),
                            ),
                        )
                        .await
                        .unwrap_or_else(|_| {
                            Err(RuntimeError::NotReady(
                                "ACP session listing timed out".to_string(),
                            ))
                        });
                        let _ = list.reply_tx.send(result);
                    }
                    control = control_rx.recv() => {
                        let Some(control) = control else { break };
                        match control {
                            AcpAgentProcessControl::Probe { agent_id, reply_tx } => {
                                let _ = reply_tx.send(Ok(agent_probe_result_from_initialize(agent_id, &initialize)));
                            }
                            AcpAgentProcessControl::Authenticate { request, reply_tx } => {
                                let result = authenticate_on_shared_process(
                                    &connection,
                                    &initialize,
                                    &config,
                                    &host_bridge,
                                    request,
                                ).await;
                                let _ = reply_tx.send(result);
                            }
                            AcpAgentProcessControl::Logout { agent_id, reply_tx } => {
                                let result = crate::agent::acp_agent_logout::logout_on_shared_process(
                                    &connection,
                                    &initialize,
                                    &agent_id,
                                ).await;
                                let _ = reply_tx.send(result);
                            }
                            AcpAgentProcessControl::Fork { request, reply_tx } => {
                                let result = fork_session_on_shared_process(
                                    &connection,
                                    &initialize,
                                    request,
                                ).await;
                                let _ = reply_tx.send(result);
                            }
                        }
                    }
                }
            }
            Ok(())
        },
    );
    let (result, selected_shutdown) = tokio::select! {
        result = connection => (result.map_err(acp_error), false),
        _ = shutdown_rx.changed() => (Ok(()), true),
    };
    // Shutdown and transport completion can become ready in the same scheduler turn. The
    // authoritative control signal wins even when `select!` observes the connection first.
    let requested_shutdown = selected_shutdown || *shutdown_rx.borrow();
    let active_session_count = diagnostic_active_session_ids
        .lock()
        .expect("ACP active session registry poisoned")
        .len();
    let active_prompt_count = diagnostic_current_prompts
        .lock()
        .expect("ACP current prompt registry poisoned")
        .len();
    let terminal = acp_connection_terminal_diagnostics(
        &result,
        requested_shutdown,
        active_session_count,
        active_prompt_count,
    );
    match &result {
        Ok(()) => logging::info(
            "acp_agent_connection_completed",
            serde_json::json!({
                "agent_id": agent_id,
                "duration_ms": connection_started_at.elapsed().as_millis(),
                "outcome_kind": terminal.outcome_kind,
                "operation": initial_operation,
                "task_id": initial_task_id,
                "exit_code": terminal.exit_code,
                "exit_signal": terminal.exit_signal,
                "active_session_count": terminal.active_session_count,
                "active_prompt_count": terminal.active_prompt_count,
            }),
        ),
        Err(_) => logging::warn(
            "acp_agent_connection_failed",
            serde_json::json!({
                "agent_id": agent_id,
                "duration_ms": connection_started_at.elapsed().as_millis(),
                "error_kind": "transport_error",
                "outcome_kind": terminal.outcome_kind,
                "operation": initial_operation,
                "task_id": initial_task_id,
                "exit_code": terminal.exit_code,
                "exit_signal": terminal.exit_signal,
                "active_session_count": terminal.active_session_count,
                "active_prompt_count": terminal.active_prompt_count,
            }),
        ),
    }
    if let (Err(error), Some(first_started_tx)) = (&result, first_started_tx) {
        let _ = first_started_tx.send(Err(RuntimeError::Internal(format!("ACP error: {error}"))));
    }
    tokio::task::spawn_blocking(move || terminal_registry.close_all())
        .await
        .map_err(|error| RuntimeError::Internal(error.to_string()))?;
    result
}

async fn fork_session_on_shared_process(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    request: AgentSessionFork,
) -> Result<AgentForkedSession, RuntimeError> {
    validate_session_fork_capabilities(initialize)?;
    let mcp_servers = match request.secret_resolver {
        Some(resolver) => {
            resolver.resolve_mcp_servers(&initialize.agent_capabilities.mcp_capabilities)?
        }
        None => Vec::new(),
    };
    let started_at = Instant::now();
    logging::info(
        "acp_session_fork_started",
        serde_json::json!({
            "agent_id": request.agent_id,
            "source_session_id": request.source_session_id,
        }),
    );
    let response = connection
        .send_request(
            ForkSessionRequest::new(request.source_session_id.clone(), request.cwd)
                .mcp_servers(mcp_servers),
        )
        .block_task()
        .await
        .map_err(acp_error)?;
    let session_id = response.session_id.to_string();
    let close_warning = connection
        .send_request(CloseSessionRequest::new(response.session_id))
        .block_task()
        .await
        .is_err();
    logging::info(
        "acp_session_fork_completed",
        serde_json::json!({
            "agent_id": request.agent_id,
            "source_session_id": request.source_session_id,
            "forked_session_id": session_id,
            "duration_ms": started_at.elapsed().as_millis(),
            "close_warning": close_warning,
        }),
    );
    Ok(AgentForkedSession {
        session_id,
        close_warning,
    })
}

async fn list_sessions_on_shared_process(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    request: AgentListSessionsRequest,
    preferred_auth_method_id: Option<&str>,
) -> Result<AgentListSessionsResult, RuntimeError> {
    let cwd = request.cwd.as_deref().map(std::path::PathBuf::from);
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
        cwd.as_deref(),
        None,
    ))
}

async fn initialize_shared_process_connection(
    connection: &ConnectionTo<Agent>,
    host_bridge: &HostBridge,
    agent_id: &str,
    first_open: Option<&AcpAgentProcessOpen>,
) -> agent_client_protocol::Result<InitializeResponse> {
    let initialize_request = initialize_request(host_bridge);
    if let Some(trace) = first_open.and_then(|open| open.trace.as_ref()) {
        trace.record("client_to_agent", "initialize.request", &initialize_request);
    }
    let Some(first_open) = first_open else {
        let started_at = Instant::now();
        logging::info(
            "acp_initialize_started",
            serde_json::json!({
                "agent_id": agent_id,
                "source": "shared_process",
            }),
        );
        let initialize = connection
            .send_request(initialize_request)
            .block_task()
            .await
            .inspect_err(|_error| {
                logging::warn(
                    "acp_initialize_failed",
                    serde_json::json!({
                        "agent_id": agent_id,
                        "duration_ms": started_at.elapsed().as_millis(),
                        "error_kind": "protocol_error",
                    }),
                );
            })?;
        if let Err(error) =
            crate::agent::acp_session_capabilities::validate_initialize_protocol(&initialize)
        {
            logging::warn(
                "acp_initialize_failed",
                serde_json::json!({
                    "agent_id": agent_id,
                    "duration_ms": started_at.elapsed().as_millis(),
                    "error_kind": "invalid_capabilities",
                }),
            );
            return Err(agent_client_protocol::util::internal_error(
                error.to_string(),
            ));
        }
        logging::info(
            "acp_initialize_completed",
            serde_json::json!({
                "agent_id": agent_id,
                "duration_ms": started_at.elapsed().as_millis(),
                "auth_method_count": initialize.auth_methods.len(),
            }),
        );
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
            let _ = first_open.started_tx.send(Err(error.clone()));
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
        session_idle_timeout,
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
        let _ = started_tx.send(Err(RuntimeError::InvalidParams(
            "agent_session_id already active".to_string(),
        )));
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
    let load_replay_for_task = Arc::clone(load_replay);
    let session_event_sinks_for_task = Arc::clone(session_event_sinks);
    let session_traces_for_task = Arc::clone(session_traces);
    let session_id_for_task = session_id.clone();
    tokio::spawn(async move {
        let result = AttachedNativeSession::run(AttachedNativeSessionRunInput {
            opened,
            request_agent_id,
            initialize,
            auth_method_id,
            load_replay: load_replay_for_task,
            command_rx,
            config_rx,
            cancel_rx,
            close_rx,
            current_prompts: current_prompts_for_task,
            trace,
            session_event_sinks: session_event_sinks_for_task,
            session_idle_timeout,
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
