use std::sync::Arc;

pub use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::acp_runtime_kernel::AcpRuntimeKernel;
use crate::agent::acp_trace::AcpTraceState;
use crate::agent::registry::AgentRegistry;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentAuthenticateRequest, AgentEventSink, AgentListSessionsRequest, AgentLoadedSession,
    AgentProbeRequest, AgentPrompt, AgentRuntime, AgentSession, AgentSessionDelete,
    AgentSessionEventSink, AgentSessionKey, AgentSessionLoad, AgentSessionResume,
    AgentSessionSetConfigOptionRequest, AgentSessionStart,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult, ConfigOptionsCatalog,
};

pub struct AcpAgentRuntime {
    kernel: AcpRuntimeKernel,
}

impl AcpAgentRuntime {
    pub fn new(config: AcpAgentConfig) -> Self {
        Self::new_with_host(config, HostBridge::disabled())
    }

    pub fn new_with_host(config: AcpAgentConfig, host_bridge: HostBridge) -> Self {
        Self::new_with_registry(
            AgentRegistryHandle::new(AgentRegistry::codex(config)),
            host_bridge,
        )
    }

    pub(crate) fn new_with_registry(
        registry: AgentRegistryHandle,
        host_bridge: HostBridge,
    ) -> Self {
        Self {
            kernel: AcpRuntimeKernel::new(registry, host_bridge),
        }
    }

    pub fn with_trace_state(mut self, trace_state: AcpTraceState) -> Self {
        self.kernel.with_trace_state(trace_state);
        self
    }

    pub fn codex() -> Self {
        Self::new(AcpAgentConfig::codex())
    }

    pub fn codex_with_host(host_bridge: HostBridge) -> Self {
        Self::new_with_host(AcpAgentConfig::codex(), host_bridge)
    }

    #[cfg(test)]
    fn probe_with_timeout(
        &self,
        request: AgentProbeRequest,
        timeout: std::time::Duration,
    ) -> Result<AgentProbeResult, RuntimeError> {
        self.kernel.probe_with_timeout(request, timeout)
    }

    #[cfg(test)]
    fn with_session_idle_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.kernel.with_session_idle_timeout(timeout);
        self
    }
}

impl AgentRuntime for AcpAgentRuntime {
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        self.kernel.probe(request)
    }

    fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        self.kernel.authenticate(request)
    }

    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        self.kernel.list_sessions(request)
    }

    fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.kernel.set_session_config_option(request)
    }

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        self.kernel.start_session(request)
    }

    fn load_session(&self, request: AgentSessionLoad) -> Result<AgentLoadedSession, RuntimeError> {
        self.kernel.load_session(request)
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        self.kernel.resume_session(request)
    }

    fn attach_session_event_sink(
        &self,
        session: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.kernel.attach_session_event_sink(session, sink)
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        self.kernel.prompt(prompt, sink)
    }

    fn steer(&self, prompt: AgentPrompt) -> Result<(), RuntimeError> {
        self.kernel.steer(prompt)
    }

    fn cancel_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.kernel.cancel_session(session)
    }

    fn close_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.kernel.close_session(session)
    }

    fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        self.kernel.delete_session(request)
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        self.kernel.shutdown()
    }
}

#[cfg(test)]
mod tests;
