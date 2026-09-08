use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::agent::acp_active_session_manager::AcpActiveSessionManager;
use crate::agent::acp_auth_method_cache::AcpAuthMethodCache;
use crate::agent::acp_runtime_threading::close_in_parallel;
use crate::agent::acp_trace::AcpTraceState;
use crate::agent::codex_acp_provisioner::CodexAcpProvisioner;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentAuthenticateRequest, AgentEventSink, AgentForkedSession, AgentListSessionsRequest,
    AgentLoadedSession, AgentProbeRequest, AgentPrompt, AgentSession, AgentSessionDelete,
    AgentSessionEventSink, AgentSessionFork, AgentSessionKey, AgentSessionLoad, AgentSessionResume,
    AgentSessionSetConfigOptionRequest, AgentSessionStart,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult, ConfigOptionsCatalog,
};

pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

pub(super) struct AcpRuntimeKernel {
    registry: AgentRegistryHandle,
    active_sessions: AcpActiveSessionManager,
    // A process belongs to one Agent identity. Never hold the registry lock while
    // waiting for ACP or a user authentication flow: unrelated Agents must progress.
    agent_process_operations: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl AcpRuntimeKernel {
    pub(super) fn new(registry: AgentRegistryHandle, host_bridge: HostBridge) -> Self {
        let auth_method_cache = AcpAuthMethodCache::default();
        let active_sessions = AcpActiveSessionManager::new(
            registry.clone(),
            host_bridge.clone(),
            auth_method_cache.clone(),
        );
        Self {
            registry,
            active_sessions,
            agent_process_operations: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn with_trace_state(&mut self, trace_state: AcpTraceState) {
        self.active_sessions.with_trace_state(trace_state);
    }

    pub(super) fn with_codex_provisioner(&mut self, provisioner: CodexAcpProvisioner) {
        self.active_sessions.with_codex_provisioner(provisioner);
    }

    pub(super) fn probe(
        &self,
        request: AgentProbeRequest,
    ) -> Result<AgentProbeResult, RuntimeError> {
        self.with_agent_process_operation(&request.agent_id, || {
            self.active_sessions.probe(&request.agent_id, PROBE_TIMEOUT)
        })
    }

    pub(super) fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        self.with_agent_process_operation(&request.agent_id.clone(), || {
            self.active_sessions.authenticate(request)
        })
    }

    pub(super) fn cancel_authentication(&self, agent_id: &str) -> Result<(), RuntimeError> {
        self.registry.require(agent_id)?;
        // Authenticate holds the process lock until Codex returns. Stop the
        // process without taking that lock so Cancel can interrupt device-code login.
        self.active_sessions.cancel_authentication(agent_id);
        Ok(())
    }

    pub(super) fn logout(&self, agent_id: &str) -> Result<(), RuntimeError> {
        self.registry.require(agent_id)?;
        self.with_agent_process_operation(agent_id, || self.active_sessions.logout(agent_id))
    }

    pub(super) fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        self.registry.require(&request.agent_id)?;

        if request
            .cwd
            .as_deref()
            .is_some_and(|cwd| !std::path::Path::new(cwd).is_absolute())
        {
            return Err(RuntimeError::InvalidParams("workspace_root".to_string()));
        }

        if !self
            .active_sessions
            .allows_passive_session_discovery(&request.agent_id)
        {
            return Ok(AgentListSessionsResult {
                agent_id: request.agent_id,
                sessions: Vec::new(),
                next_cursor: None,
            });
        }

        self.with_agent_process_operation(&request.agent_id.clone(), || {
            self.active_sessions.list_sessions(request)
        })
    }

    pub(super) fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.registry.require(&request.agent_id)?;
        self.active_sessions.set_session_config_option(request)
    }

    pub(super) fn start_session(
        &self,
        request: AgentSessionStart,
    ) -> Result<AgentSession, RuntimeError> {
        self.with_agent_process_operation(&request.agent_id.clone(), || {
            self.active_sessions.start_session(request)
        })
    }

    pub(super) fn load_session(
        &self,
        request: AgentSessionLoad,
    ) -> Result<AgentLoadedSession, RuntimeError> {
        self.with_agent_process_operation(&request.agent_id.clone(), || {
            self.active_sessions.load_session(request)
        })
    }

    pub(super) fn resume_session(
        &self,
        request: AgentSessionResume,
    ) -> Result<AgentSession, RuntimeError> {
        // Reading a live attachment is session-local: background discovery must
        // not hold its controls behind the Agent process lifecycle lock. A missing
        // attachment still acquires that lock and rechecks before opening, so
        // concurrent resumes cannot create duplicate attachments.
        if let Some(snapshot) = self
            .active_sessions
            .snapshot_attached_session(&request.session_key())
        {
            return snapshot;
        }
        self.with_agent_process_operation(&request.agent_id.clone(), || {
            self.active_sessions.resume_session(request)
        })
    }

    pub(super) fn attach_session_event_sink(
        &self,
        session: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.active_sessions
            .attach_session_event_sink(session, sink)
    }

    pub(super) fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        self.active_sessions.prompt(prompt, sink)
    }

    pub(super) fn steer(&self, prompt: AgentPrompt) -> Result<(), RuntimeError> {
        self.active_sessions.steer(prompt)
    }

    pub(super) fn cancel_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.active_sessions.cancel_session(session)
    }

    pub(super) fn close_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.active_sessions.close_session(session)
    }

    pub(super) fn fork_session(
        &self,
        request: AgentSessionFork,
    ) -> Result<AgentForkedSession, RuntimeError> {
        self.registry.require(&request.agent_id)?;
        if !std::path::Path::new(&request.cwd).is_absolute() {
            return Err(RuntimeError::InvalidParams("workspace_root".to_string()));
        }
        self.with_agent_process_operation(&request.agent_id.clone(), || {
            self.active_sessions.fork_session(request)
        })
    }

    pub(super) fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        self.active_sessions.delete_session(request)
    }

    pub(super) fn shutdown(&self) -> Result<(), RuntimeError> {
        let mut close_tasks: Vec<Box<dyn FnOnce() + Send + 'static>> = Vec::new();
        close_tasks.extend(self.active_sessions.take_shutdown_close_tasks());
        close_in_parallel(close_tasks);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn probe_with_timeout(
        &self,
        request: AgentProbeRequest,
        timeout: Duration,
    ) -> Result<AgentProbeResult, RuntimeError> {
        self.with_agent_process_operation(&request.agent_id, || {
            self.active_sessions.probe(&request.agent_id, timeout)
        })
    }

    #[cfg(test)]
    pub(super) fn with_session_idle_timeout(&mut self, timeout: Duration) {
        self.active_sessions.with_session_idle_timeout(timeout);
    }

    #[cfg(test)]
    pub(super) fn with_process_idle_timeouts(&mut self, short: Duration, long: Duration) {
        self.active_sessions.with_process_idle_timeouts(short, long);
    }

    #[cfg(test)]
    pub(super) fn with_list_timeout(&mut self, timeout: Duration) {
        self.active_sessions.with_list_timeout(timeout);
    }

    fn with_agent_process_operation<T>(
        &self,
        agent_id: &str,
        operation: impl FnOnce() -> Result<T, RuntimeError>,
    ) -> Result<T, RuntimeError> {
        let agent_operation = self
            .agent_process_operations
            .lock()
            .map_err(|_| {
                RuntimeError::Internal("ACP process operation registry poisoned".to_string())
            })?
            .entry(agent_id.to_string())
            .or_default()
            .clone();
        let _operation = agent_operation.lock().map_err(|_| {
            RuntimeError::Internal("ACP Agent process operation lock poisoned".to_string())
        })?;
        operation()
    }
}
