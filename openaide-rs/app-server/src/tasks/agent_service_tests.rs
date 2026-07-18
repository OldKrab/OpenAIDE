use super::*;
use crate::agent::{AgentEventSink, AgentPrompt, AgentRuntime, AgentSession, AgentSessionStart};
use crate::protocol::model::{AgentProbeCapabilities, AgentProbeStatus};
use openaide_app_server_protocol::snapshot::AgentStatus;
use std::sync::Arc;

#[test]
fn probe_success_updates_shared_agent_status_cache() {
    let statuses = AgentStatusCache::default();
    let service = AgentService::with_status_cache(
        AgentGateway::new(Arc::new(ReadyAgent)),
        AgentRegistry::default_built_ins(),
        statuses.clone(),
    );

    service
        .probe(AgentProbeParams {
            agent_id: "codex".to_string(),
        })
        .unwrap();

    let snapshot = statuses.snapshot("codex");
    assert_eq!(snapshot.status, AgentStatus::Connected);
    assert!(snapshot.capabilities.resume_tasks);
}

#[test]
fn probe_failure_updates_shared_agent_status_cache() {
    let statuses = AgentStatusCache::default();
    let service = AgentService::with_status_cache(
        AgentGateway::new(Arc::new(AuthRequiredAgent)),
        AgentRegistry::default_built_ins(),
        statuses.clone(),
    );

    let error = service
        .probe(AgentProbeParams {
            agent_id: "codex".to_string(),
        })
        .unwrap_err();

    assert!(matches!(error, RuntimeError::AuthRequired(_)));
    assert_eq!(statuses.snapshot("codex").status, AgentStatus::AuthRequired);
}

struct ReadyAgent;

impl AgentRuntime for ReadyAgent {
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Ok(AgentProbeResult {
            agent_id: request.agent_id,
            status: AgentProbeStatus::Ready,
            protocol_version: "1".to_string(),
            implementation_name: None,
            implementation_version: None,
            capabilities: vec!["Resume sessions".to_string()],
            typed_capabilities: AgentProbeCapabilities {
                resume_sessions: true,
                delete_sessions: false,
            },
            auth_methods: Vec::new(),
            logout_supported: false,
        })
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Err(RuntimeError::CapabilityMissing("test".to_string()))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Err(RuntimeError::CapabilityMissing("test".to_string()))
    }
}

struct AuthRequiredAgent;

impl AgentRuntime for AuthRequiredAgent {
    fn probe(&self, _request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Err(RuntimeError::AuthRequired(
            "Authentication required".to_string(),
        ))
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Err(RuntimeError::CapabilityMissing("test".to_string()))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Err(RuntimeError::CapabilityMissing("test".to_string()))
    }
}
