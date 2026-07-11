use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::agent::acp_active_session_manager::AcpActiveSessionManager;
use crate::agent::acp_auth_method_cache::AcpAuthMethodCache;
use crate::agent::acp_options_session_manager::AcpOptionsSessionManager;
use crate::agent::acp_probe_auth_runner::AcpProbeAuthRunner;
use crate::agent::acp_runtime_threading::close_in_parallel;
use crate::agent::acp_trace::AcpTraceState;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentAuthenticateRequest, AgentConfigOptionsRequest, AgentEventSink, AgentListSessionsRequest,
    AgentLoadedSession, AgentProbeRequest, AgentPrompt, AgentSession, AgentSessionDelete,
    AgentSessionEventSink, AgentSessionLoad, AgentSessionResume,
    AgentSessionSetConfigOptionRequest, AgentSessionStart, AgentSetConfigOptionRequest,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult, ConfigOptionsCatalog,
};

pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const AUTHENTICATE_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) struct AcpRuntimeKernel {
    registry: AgentRegistryHandle,
    probe_auth: AcpProbeAuthRunner,
    options_sessions: AcpOptionsSessionManager,
    active_sessions: AcpActiveSessionManager,
}

impl AcpRuntimeKernel {
    pub(super) fn new(registry: AgentRegistryHandle, host_bridge: HostBridge) -> Self {
        let auth_method_cache = AcpAuthMethodCache::default();
        let options_sessions = AcpOptionsSessionManager::new(
            registry.clone(),
            host_bridge.clone(),
            auth_method_cache.clone(),
        );
        let active_sessions = AcpActiveSessionManager::new(
            registry.clone(),
            host_bridge.clone(),
            auth_method_cache.clone(),
        );
        let probe_auth = AcpProbeAuthRunner::new(
            registry.clone(),
            host_bridge.clone(),
            auth_method_cache.clone(),
        );
        Self {
            registry,
            probe_auth,
            options_sessions,
            active_sessions,
        }
    }

    pub(super) fn with_trace_state(&mut self, trace_state: AcpTraceState) {
        self.active_sessions.with_trace_state(trace_state);
    }

    pub(super) fn probe(
        &self,
        request: AgentProbeRequest,
    ) -> Result<AgentProbeResult, RuntimeError> {
        self.probe_with_timeout(request, PROBE_TIMEOUT)
    }

    pub(super) fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        self.authenticate_with_timeout(request, AUTHENTICATE_TIMEOUT)
    }

    pub(super) fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        self.registry.require(&request.agent_id)?;

        let cwd = PathBuf::from(request.cwd.clone());
        if !cwd.is_absolute() {
            return Err(RuntimeError::InvalidParams("workspace_root".to_string()));
        }

        self.options_sessions
            .with_options_session(&request.agent_id, &request.cwd, |session| {
                session.list_sessions(
                    request.agent_id.clone(),
                    cwd.clone(),
                    request.cursor.clone(),
                )
            })
    }

    pub(super) fn config_options(
        &self,
        request: AgentConfigOptionsRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.registry.require(&request.agent_id)?;
        self.options_sessions
            .with_options_session(&request.agent_id, &request.cwd, |session| {
                session.config_options()
            })
    }

    pub(super) fn set_config_option(
        &self,
        request: AgentSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.registry.require(&request.agent_id)?;
        let config_id = request.config_id.clone();
        let value = request.value.clone();
        self.options_sessions
            .with_options_session(&request.agent_id, &request.cwd, |session| {
                session.set_config_option(config_id.clone(), value.clone())
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
        self.active_sessions.start_session(request)
    }

    pub(super) fn load_session(
        &self,
        request: AgentSessionLoad,
    ) -> Result<AgentLoadedSession, RuntimeError> {
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
        session_id: &str,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.active_sessions
            .attach_session_event_sink(session_id, sink)
    }

    pub(super) fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        self.active_sessions.prompt(prompt, sink)
    }

    pub(super) fn cancel_session(&self, session_id: &str) -> Result<(), RuntimeError> {
        self.active_sessions.cancel_session(session_id)
    }

    pub(super) fn close_session(&self, session_id: &str) -> Result<(), RuntimeError> {
        self.active_sessions.close_session(session_id)
    }

    pub(super) fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        self.active_sessions.delete_session(request)
    }

    pub(super) fn shutdown(&self) -> Result<(), RuntimeError> {
        let mut close_tasks: Vec<Box<dyn FnOnce() + Send + 'static>> = Vec::new();
        if let Some(task) = self.options_sessions.take_shutdown_close_task() {
            close_tasks.push(task);
        }
        close_tasks.extend(self.active_sessions.take_shutdown_close_tasks());
        close_in_parallel(close_tasks);
        Ok(())
    }

    pub(super) fn probe_with_timeout(
        &self,
        request: AgentProbeRequest,
        timeout: Duration,
    ) -> Result<AgentProbeResult, RuntimeError> {
        self.probe_auth.probe_with_timeout(request, timeout)
    }

    fn authenticate_with_timeout(
        &self,
        request: AgentAuthenticateRequest,
        timeout: Duration,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        self.probe_auth.authenticate_with_timeout(request, timeout)
    }
}
