use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::agent::acp_active_session_manager::AcpActiveSessionManager;
use crate::agent::acp_auth_method_cache::AcpAuthMethodCache;
use crate::agent::acp_runtime_threading::close_in_parallel;
use crate::agent::acp_trace::AcpTraceState;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentAuthenticateRequest, AgentEventSink, AgentListSessionsRequest, AgentLoadedSession,
    AgentProbeRequest, AgentPrompt, AgentSession, AgentSessionDelete, AgentSessionEventSink,
    AgentSessionKey, AgentSessionLoad, AgentSessionResume, AgentSessionSetConfigOptionRequest,
    AgentSessionStart,
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
    agent_process_operations: Mutex<()>,
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
            agent_process_operations: Mutex::new(()),
        }
    }

    pub(super) fn with_trace_state(&mut self, trace_state: AcpTraceState) {
        self.active_sessions.with_trace_state(trace_state);
    }

    pub(super) fn probe(
        &self,
        request: AgentProbeRequest,
    ) -> Result<AgentProbeResult, RuntimeError> {
        let _operation = self.lock_agent_process_operations()?;
        self.active_sessions.probe(&request.agent_id, PROBE_TIMEOUT)
    }

    pub(super) fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        let _operation = self.lock_agent_process_operations()?;
        self.active_sessions.authenticate(request)
    }

    pub(super) fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        let _operation = self.lock_agent_process_operations()?;
        self.registry.require(&request.agent_id)?;

        let cwd = std::path::PathBuf::from(request.cwd.clone());
        if !cwd.is_absolute() {
            return Err(RuntimeError::InvalidParams("workspace_root".to_string()));
        }

        self.active_sessions.list_sessions(request)
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
        let _operation = self.lock_agent_process_operations()?;
        self.active_sessions.start_session(request)
    }

    pub(super) fn load_session(
        &self,
        request: AgentSessionLoad,
    ) -> Result<AgentLoadedSession, RuntimeError> {
        let _operation = self.lock_agent_process_operations()?;
        self.active_sessions.load_session(request)
    }

    pub(super) fn resume_session(
        &self,
        request: AgentSessionResume,
    ) -> Result<AgentSession, RuntimeError> {
        self.active_sessions.resume_session(request)
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
    ) -> Result<(), RuntimeError> {
        self.active_sessions.prompt(prompt, sink)
    }

    pub(super) fn cancel_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.active_sessions.cancel_session(session)
    }

    pub(super) fn close_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.active_sessions.close_session(session)
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
        let _operation = self.lock_agent_process_operations()?;
        self.active_sessions.probe(&request.agent_id, timeout)
    }

    fn lock_agent_process_operations(&self) -> Result<std::sync::MutexGuard<'_, ()>, RuntimeError> {
        self.agent_process_operations
            .lock()
            .map_err(|_| RuntimeError::Internal("ACP process operation lock poisoned".to_string()))
    }
}
