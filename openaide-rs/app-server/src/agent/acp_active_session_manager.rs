use std::sync::{mpsc, Arc};
use std::time::Duration;

use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_active_session_registry::AcpActiveSessionRegistry;
use crate::agent::acp_agent_process_pool::AcpAgentProcessPool;
use crate::agent::acp_auth_method_cache::AcpAuthMethodCache;
use crate::agent::acp_host_terminal_ownership::AcpTerminalOwnerId;
use crate::agent::acp_session_client::AcpSessionClient;
use crate::agent::acp_session_worker::{
    AcpAgentProcessOpen, AcpSessionOpenRequest, AcpStartedSession,
};
use crate::agent::acp_trace::{AcpTraceSession, AcpTraceState};
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentAuthenticateRequest, AgentEventSink, AgentListSessionsRequest, AgentLoadedSession,
    AgentPrompt, AgentSession, AgentSessionDelete, AgentSessionEventSink, AgentSessionKey,
    AgentSessionLoad, AgentSessionResume, AgentSessionSetConfigOptionRequest, AgentSessionStart,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult, ConfigOptionsCatalog,
};

const DEFAULT_START_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct AcpActiveSessionManager {
    auth_method_cache: AcpAuthMethodCache,
    trace_state: AcpTraceState,
    start_timeout: Duration,
    sessions: AcpActiveSessionRegistry,
    processes: AcpAgentProcessPool,
}

impl AcpActiveSessionManager {
    pub(super) fn new(
        registry: impl Into<AgentRegistryHandle>,
        host_bridge: HostBridge,
        auth_method_cache: AcpAuthMethodCache,
    ) -> Self {
        let registry = registry.into();
        Self {
            auth_method_cache,
            trace_state: AcpTraceState::disabled(std::path::Path::new(".")),
            start_timeout: DEFAULT_START_TIMEOUT,
            sessions: AcpActiveSessionRegistry::new(),
            processes: AcpAgentProcessPool::new(registry, host_bridge),
        }
    }

    pub(super) fn with_trace_state(&mut self, trace_state: AcpTraceState) {
        self.trace_state = trace_state;
    }

    #[cfg(test)]
    pub(super) fn with_start_timeout(&mut self, start_timeout: Duration) {
        self.start_timeout = start_timeout;
    }

    pub(super) fn start_session(
        &self,
        request: AgentSessionStart,
    ) -> Result<AgentSession, RuntimeError> {
        if request.cancellation.is_cancelled() {
            return Err(RuntimeError::InvalidParams("session cancelled".to_string()));
        }

        let started = self.open_session(AcpSessionOpenRequest::Start(request))?;
        Ok(started.session)
    }

    pub(super) fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        self.processes
            .list_sessions(request, self.auth_method_cache.preferred_method())
    }

    pub(super) fn probe(
        &self,
        agent_id: &str,
        timeout: Duration,
    ) -> Result<AgentProbeResult, RuntimeError> {
        self.processes.probe(agent_id, timeout)
    }

    pub(super) fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        let result = self.processes.authenticate(request)?;
        self.auth_method_cache
            .record_authenticated_method(result.method_id.clone());
        Ok(result)
    }

    pub(super) fn load_session(
        &self,
        request: AgentSessionLoad,
    ) -> Result<AgentLoadedSession, RuntimeError> {
        if request.cancellation.is_cancelled() {
            return Err(RuntimeError::InvalidParams("session cancelled".to_string()));
        }

        let started = self.open_session(AcpSessionOpenRequest::Load(request))?;
        Ok(AgentLoadedSession {
            session: started.session,
            replayed_messages: started.replayed_messages,
        })
    }

    pub(super) fn resume_session(
        &self,
        request: AgentSessionResume,
    ) -> Result<AgentSession, RuntimeError> {
        let session = request.session_key();
        if self.sessions.contains(&session) {
            return Ok(AgentSession::new(request.agent_id, request.session_id));
        }
        Err(RuntimeError::CapabilityMissing(
            "acp_session_resume_after_runtime_restart".to_string(),
        ))
    }

    pub(super) fn attach_session_event_sink(
        &self,
        session: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.sessions.attach_session_event_sink(session, sink)
    }

    pub(super) fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        let session = request.session_key();
        self.sessions
            .set_config_option(&session, request.config_id, request.value)
    }

    pub(super) fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        self.sessions.prompt(prompt, sink)
    }

    pub(super) fn cancel_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.sessions.cancel_session(session)
    }

    pub(super) fn close_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.sessions.close_session(session)
    }

    pub(super) fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        self.sessions.delete_session(request)
    }

    pub(super) fn take_shutdown_close_tasks(&self) -> Vec<Box<dyn FnOnce() + Send + 'static>> {
        self.sessions.take_shutdown_close_tasks()
    }

    fn open_session(
        &self,
        request: AcpSessionOpenRequest,
    ) -> Result<AcpStartedSession, RuntimeError> {
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (config_tx, config_rx) = tokio_mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = tokio_mpsc::unbounded_channel();
        let (close_tx, close_rx) = tokio_mpsc::unbounded_channel();
        let (started_tx, started_rx) = mpsc::channel();
        let trace = Some(AcpTraceSession::new(
            self.trace_state.clone(),
            request.task_id(),
            request.operation_name(),
        ));
        let startup_cancellation = request.cancellation();
        let auth_method_id = self.auth_method_cache.preferred_method();
        let agent_id = request.agent_id().to_string();
        let process_open = AcpAgentProcessOpen {
            request,
            command_rx,
            config_rx,
            cancel_rx,
            close_rx,
            started_tx,
            auth_method_id,
            trace,
            terminal_owner_id: AcpTerminalOwnerId::next(),
        };
        let process_session = self.processes.open_session(&agent_id, process_open)?;

        let started_result = match started_rx.recv_timeout(self.start_timeout) {
            Ok(result) => result.map_err(startup_open_error),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                startup_cancellation.cancel();
                let _ = process_session.terminal_owner.close();
                close_starting_session(&close_tx);
                Err(RuntimeError::NotReady(
                    "ACP session start timed out".to_string(),
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeError::NotReady(
                "ACP session ended before startup completed".to_string(),
            )),
        };
        let started = match started_result {
            Ok(started) => started,
            Err(error) => {
                let _ = process_session.terminal_owner.close();
                return Err(error);
            }
        };

        let session_client = AcpSessionClient::new(
            command_tx,
            config_tx,
            cancel_tx,
            close_tx,
            process_session.terminal_error,
            process_session.terminal_owner,
        );
        self.sessions
            .insert_started_session(started.session.key(), session_client)?;

        Ok(started)
    }
}

fn startup_open_error(message: String) -> RuntimeError {
    if message == "agent_session_id already active" {
        RuntimeError::InvalidParams(message)
    } else {
        RuntimeError::NotReady(message)
    }
}

fn close_starting_session(
    close_tx: &tokio_mpsc::UnboundedSender<mpsc::Sender<Result<(), RuntimeError>>>,
) {
    let (reply_tx, reply_rx) = mpsc::channel();
    if close_tx.send(reply_tx).is_ok() {
        let _ = reply_rx.recv_timeout(Duration::from_secs(2));
    }
}
