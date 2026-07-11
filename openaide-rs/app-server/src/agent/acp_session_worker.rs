use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, Mutex};

use agent_client_protocol::schema::InitializeResponse;
use agent_client_protocol::{Agent, ConnectionTo};

use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::acp_host::initialize_request;
use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwnerId};
use crate::agent::acp_session_runner::{acp_start_error, initialize_agent_connection};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::{
    AgentSecretResolver, AgentSession, AgentSessionLoad, AgentSessionStart, TurnCancellation,
};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::NormalizedMessage;

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_opened_session_worker::{
    run_opened_acp_session, AcpOpenedSessionWorkerInput,
};
use crate::agent::acp_session_client::{AcpSessionCommand, AcpSessionConfigCommand};
use crate::agent::acp_session_connection::{
    connect_acp_session_client, AcpSessionConnectionContext,
};
use crate::agent::acp_session_lifecycle::LoadReplayCapture;
use crate::agent::acp_session_opening::{open_acp_session, OpenAcpSessionContext};
use crate::agent::acp_session_termination::close_active_session;
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
    pub(super) first_open: AcpAgentProcessOpen,
    pub(super) open_rx: tokio_mpsc::UnboundedReceiver<AcpAgentProcessOpen>,
    pub(super) host_bridge: HostBridge,
    pub(super) terminal_registry: AcpHostTerminalRegistry,
}

pub(super) async fn run_acp_agent_process(input: AcpAgentProcessInput) -> Result<(), RuntimeError> {
    let AcpAgentProcessInput {
        config,
        first_open,
        mut open_rx,
        host_bridge,
        terminal_registry,
    } = input;

    let current_prompts: Arc<Mutex<HashMap<String, LivePromptProjection>>> = Arc::default();
    let load_replay: Arc<Mutex<HashMap<String, LoadReplayCapture>>> = Arc::default();
    let active_session_ids: Arc<Mutex<HashSet<String>>> = Arc::default();
    let session_event_sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap =
        Arc::default();
    let elicitation_cancellations: crate::agent::acp_host_capabilities::AcpElicitationCancellationMap =
        Arc::default();
    let first_started_tx = first_open.started_tx.clone();
    terminal_registry.begin_open(first_open.terminal_owner_id);
    let agent = match config.to_acp_agent(
        first_open.trace.clone(),
        &host_bridge,
        first_open.request.secret_resolver(),
    ) {
        Ok(agent) => agent,
        Err(error) => {
            let _ = first_open.started_tx.send(Err(error.to_string()));
            return Err(error);
        }
    };
    let connection_context = AcpSessionConnectionContext {
        host_bridge: host_bridge.clone(),
        trace: first_open.trace.clone(),
        current_prompts: current_prompts.clone(),
        load_replay: load_replay.clone(),
        terminal_registry: terminal_registry.clone(),
        session_event_sinks: session_event_sinks.clone(),
        elicitation_cancellations,
    };
    let connection_terminal_registry = terminal_registry.clone();

    let result = connect_acp_session_client(
        agent,
        connection_context,
        |connection: ConnectionTo<Agent>| async move {
            let initialize =
                initialize_shared_process_connection(&connection, &host_bridge, &first_open)
                    .await?;
            open_on_shared_process(
                &connection,
                initialize.clone(),
                &host_bridge,
                &current_prompts,
                &load_replay,
                &active_session_ids,
                &connection_terminal_registry,
                &session_event_sinks,
                first_open,
            )
            .await?;
            while let Some(open) = open_rx.recv().await {
                if let Err(error) = open_on_shared_process(
                    &connection,
                    initialize.clone(),
                    &host_bridge,
                    &current_prompts,
                    &load_replay,
                    &active_session_ids,
                    &connection_terminal_registry,
                    &session_event_sinks,
                    open,
                )
                .await
                {
                    logging::warn(
                        "acp_shared_session_open_failed",
                        serde_json::json!({ "error": error.to_string() }),
                    );
                }
            }
            Ok(())
        },
    )
    .await
    .map_err(acp_error);
    if let Err(error) = &result {
        let _ = first_started_tx.send(Err(format!("ACP error: {error}")));
    }
    tokio::task::spawn_blocking(move || terminal_registry.close_all())
        .await
        .map_err(|error| RuntimeError::Internal(error.to_string()))?;
    result
}

async fn initialize_shared_process_connection(
    connection: &ConnectionTo<Agent>,
    host_bridge: &HostBridge,
    first_open: &AcpAgentProcessOpen,
) -> agent_client_protocol::Result<InitializeResponse> {
    let initialize_request = initialize_request(host_bridge);
    let cancellation = first_open.request.cancellation();
    if let Some(trace) = &first_open.trace {
        trace.record("client_to_agent", "initialize.request", &initialize_request);
    }
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
        initialize: Some(initialize),
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
        let connection = opened.active_session.connection();
        close_active_session(
            &connection,
            opened.active_session.session_id().clone(),
            opened.supports_session_close,
            trace.as_ref(),
        )
        .await;
        let owner = terminal_owner.clone();
        let _ = tokio::task::spawn_blocking(move || owner.close()).await;
        let _ = started_tx.send(Err("agent_session_id already active".to_string()));
        return Ok(());
    }
    let active_session_ids_for_task = active_session_ids.clone();
    let current_prompts_for_task = current_prompts.clone();
    let session_event_sinks_for_task = Arc::clone(session_event_sinks);
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
        result
    });
    let _ = started_tx.send(Ok(AcpStartedSession {
        session: started_session,
        replayed_messages,
    }));
    Ok(())
}

async fn wait_for_shared_startup_cancellation(cancellation: TurnCancellation) -> RuntimeError {
    loop {
        if cancellation.is_cancelled() {
            return RuntimeError::NotReady("ACP session start cancelled".to_string());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
