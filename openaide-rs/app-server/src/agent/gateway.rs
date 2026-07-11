use std::sync::Arc;

use crate::agent::{
    AgentAuthenticateRequest, AgentConfigOptionsRequest, AgentListSessionsRequest,
    AgentLoadedSession, AgentProbeRequest, AgentRuntime, AgentSession, AgentSessionLoad,
    AgentSessionResume, AgentSessionSetConfigOptionRequest, AgentSessionStart,
    AgentSetConfigOptionRequest,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult, ConfigOptionsCatalog,
};
use crate::tasks::native_session_lifecycle::NativeSessionLifecycle;

#[derive(Clone)]
pub(crate) struct AgentGateway {
    agent: Arc<dyn AgentRuntime>,
}

impl AgentGateway {
    pub(crate) fn new(agent: Arc<dyn AgentRuntime>) -> Self {
        Self { agent }
    }

    pub(crate) fn probe(
        &self,
        request: AgentProbeRequest,
    ) -> Result<AgentProbeResult, RuntimeError> {
        self.agent.probe(request)
    }

    pub(crate) fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        self.agent.authenticate(request)
    }

    pub(crate) fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        self.agent.list_sessions(request)
    }

    pub(crate) fn config_options(
        &self,
        request: AgentConfigOptionsRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.agent.config_options(request)
    }

    pub(crate) fn set_config_option(
        &self,
        request: AgentSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.agent.set_config_option(request)
    }

    pub(crate) fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.agent.set_session_config_option(request)
    }

    pub(crate) fn start_session(
        &self,
        request: AgentSessionStart,
    ) -> Result<AgentSession, RuntimeError> {
        self.agent.start_session(request)
    }

    pub(crate) fn load_session(
        &self,
        request: AgentSessionLoad,
    ) -> Result<AgentLoadedSession, RuntimeError> {
        self.agent.load_session(request)
    }

    pub(crate) fn resume_session(
        &self,
        request: AgentSessionResume,
    ) -> Result<AgentSession, RuntimeError> {
        self.agent.resume_session(request)
    }

    pub(crate) fn close_session(&self, session_id: &str) -> Result<(), RuntimeError> {
        self.agent.close_session(session_id)
    }

    pub(crate) fn native_session_lifecycle(&self) -> NativeSessionLifecycle<'_> {
        NativeSessionLifecycle::new(self.agent.as_ref())
    }
}
