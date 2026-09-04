use std::sync::Arc;

use crate::agent::status_cache::AgentStatusCache;
use crate::agent::{
    AgentAuthenticateRequest, AgentEventSink, AgentForkedSession, AgentListSessionsRequest,
    AgentLoadedSession, AgentProbeRequest, AgentPrompt, AgentRuntime, AgentSession,
    AgentSessionDelete, AgentSessionEventSink, AgentSessionFork, AgentSessionKey, AgentSessionLoad,
    AgentSessionResume, AgentSessionSetConfigOptionRequest, AgentSessionStart,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult, ConfigOptionsCatalog,
};
use openaide_app_server_protocol::snapshot::AgentStatus;

/// Records Agent Status from the same ACP process work Tasks already use.
pub(crate) struct AgentStatusRecordingRuntime {
    inner: Arc<dyn AgentRuntime>,
    statuses: AgentStatusCache,
}

impl AgentStatusRecordingRuntime {
    pub(crate) fn wrap(
        inner: Arc<dyn AgentRuntime>,
        statuses: AgentStatusCache,
    ) -> Arc<dyn AgentRuntime> {
        Arc::new(Self { inner, statuses })
    }

    fn record_session_outcome<T>(&self, agent_id: &str, result: &Result<T, RuntimeError>) {
        match result {
            Ok(_) => self.statuses.record_connected(agent_id),
            Err(error) => self.statuses.record_session_error(agent_id, error),
        }
    }
}

impl AgentRuntime for AgentStatusRecordingRuntime {
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        let agent_id = request.agent_id.clone();
        let result = self.inner.probe(request);
        match &result {
            Ok(probe) => self.statuses.record_probe_success(probe),
            Err(error) => self.statuses.record_probe_error(&agent_id, error),
        }
        result
    }

    fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        self.inner.authenticate(request)
    }

    fn cancel_authentication(&self, agent_id: &str) -> Result<(), RuntimeError> {
        self.inner.cancel_authentication(agent_id)
    }

    fn logout(&self, agent_id: &str) -> Result<(), RuntimeError> {
        self.inner.logout(agent_id)
    }

    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        let agent_id = request.agent_id.clone();
        let result = self.inner.list_sessions(request);
        let snapshot = self.statuses.snapshot(&agent_id);
        if snapshot.auth_methods.is_empty()
            || matches!(
                snapshot.status,
                AgentStatus::Launching | AgentStatus::Installing | AgentStatus::Disconnected
            )
        {
            let _ = self.probe(AgentProbeRequest {
                agent_id: agent_id.clone(),
            });
        }
        self.record_session_outcome(&agent_id, &result);
        result
    }

    fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.inner.set_session_config_option(request)
    }

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        let agent_id = request.agent_id.clone();
        let result = self.inner.start_session(request);
        self.record_session_outcome(&agent_id, &result);
        result
    }

    fn load_session(&self, request: AgentSessionLoad) -> Result<AgentLoadedSession, RuntimeError> {
        let agent_id = request.agent_id.clone();
        let result = self.inner.load_session(request);
        self.record_session_outcome(&agent_id, &result);
        result
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        let agent_id = request.agent_id.clone();
        let result = self.inner.resume_session(request);
        self.record_session_outcome(&agent_id, &result);
        result
    }

    fn attach_session_event_sink(
        &self,
        session: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.inner.attach_session_event_sink(session, sink)
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        self.inner.prompt(prompt, sink)
    }

    fn steer(&self, prompt: AgentPrompt) -> Result<(), RuntimeError> {
        self.inner.steer(prompt)
    }

    fn cancel_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.inner.cancel_session(session)
    }

    fn close_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.inner.close_session(session)
    }

    fn fork_session(&self, request: AgentSessionFork) -> Result<AgentForkedSession, RuntimeError> {
        self.inner.fork_session(request)
    }

    fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        self.inner.delete_session(request)
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        self.inner.shutdown()
    }
}

#[cfg(test)]
#[path = "status_recording_runtime_tests.rs"]
mod tests;
