use super::*;
use crate::agent::registry::AgentRegistry;
use crate::agent::{AgentEventSink, AgentPrompt, AgentRuntime, AgentSession, AgentSessionStart};
use crate::protocol::model::{AgentProbeCapabilities, AgentProbeStatus};
use crate::storage::Store;
use openaide_app_server_protocol::agent::{
    AgentAuthenticateParams, AgentAuthenticateStatus as ProtocolAgentAuthenticateStatus,
    AgentCreateCustomParams, AgentDeleteCustomParams, AgentReplaceCustomConfirmation,
    AgentReplaceCustomHistoryPolicy, AgentReplaceCustomParams,
};
use openaide_app_server_protocol::ids::AgentId;
use openaide_app_server_protocol::snapshot::AgentStatus;
use std::collections::BTreeMap;
use std::sync::Arc;

#[test]
fn probe_success_returns_updated_agent_collection() {
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(ReadyAgent),
        AgentStatusCache::default(),
    );

    let result = api
        .probe(ProtocolAgentProbeParams {
            agent_id: AgentId::from("codex"),
        })
        .unwrap();

    let codex = result
        .agents
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == "codex")
        .unwrap();
    assert_eq!(codex.status, AgentStatus::Connected);
    assert!(codex.capabilities.resume_tasks);
}

#[test]
fn expected_probe_failure_returns_updated_agent_collection() {
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(AuthRequiredAgent),
        AgentStatusCache::default(),
    );

    let result = api
        .probe(ProtocolAgentProbeParams {
            agent_id: AgentId::from("codex"),
        })
        .unwrap();

    let codex = result
        .agents
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == "codex")
        .unwrap();
    assert_eq!(codex.status, AgentStatus::AuthRequired);
}

#[test]
fn internal_probe_failure_updates_cache_and_returns_protocol_error() {
    let statuses = AgentStatusCache::default();
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(InternalFailingAgent),
        statuses.clone(),
    );

    let error = api
        .probe(ProtocolAgentProbeParams {
            agent_id: AgentId::from("codex"),
        })
        .unwrap_err();

    assert_eq!(
        error.code,
        openaide_app_server_protocol::errors::ProtocolErrorCode::Internal
    );
    assert_eq!(statuses.snapshot("codex").status, AgentStatus::Failed);
}

#[test]
fn explicit_authentication_clears_auth_required_status() {
    let statuses = AgentStatusCache::default();
    statuses.record_probe_error(
        "codex",
        &RuntimeError::AuthRequired("Authentication required".to_string()),
    );
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(ExplicitAuthAgent),
        statuses.clone(),
    );

    let result = api
        .authenticate(AgentAuthenticateParams {
            agent_id: AgentId::from("codex"),
            method_id: "browser-login".to_string(),
            env: BTreeMap::new(),
            secret_env: Vec::new(),
            secret_storage_agent_id: None,
            terminal_confirmed: false,
        })
        .unwrap();

    assert_eq!(
        result.status,
        ProtocolAgentAuthenticateStatus::Authenticated
    );
    assert_eq!(statuses.snapshot("codex").status, AgentStatus::Connected);
}

#[test]
fn authentication_failure_does_not_expose_agent_error_details() {
    let statuses = AgentStatusCache::default();
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(FailingAuthAgent),
        statuses.clone(),
    );

    let error = api
        .authenticate(AgentAuthenticateParams {
            agent_id: AgentId::from("codex"),
            method_id: "api-key".to_string(),
            env: BTreeMap::new(),
            secret_env: Vec::new(),
            secret_storage_agent_id: None,
            terminal_confirmed: false,
        })
        .unwrap_err();

    assert_eq!(
        error.message,
        "Authentication failed. Check the Agent's requirements and try again."
    );
    assert!(!error.message.contains("CODEX_API_KEY"));
    assert_eq!(statuses.snapshot("codex").status, AgentStatus::Disconnected);
}

#[test]
fn custom_agent_replacement_reports_cleanup_and_preserves_history_policy() {
    let statuses = AgentStatusCache::default();
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(ReadyAgent),
        statuses.clone(),
    );
    let created = api
        .create_custom(AgentCreateCustomParams {
            agent_id: None,
            label: "Local Agent".to_string(),
            icon: "bot".to_string(),
            command_line: "local-agent".to_string(),
            command: "local-agent".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            secret_env: vec!["OLD_TOKEN".to_string()],
            enabled: true,
        })
        .unwrap();
    statuses.record_probe_error(
        created.agent_id.as_str(),
        &RuntimeError::AuthRequired("Authentication required".to_string()),
    );

    let replaced = api
        .replace_custom(AgentReplaceCustomParams {
            source_agent_id: created.agent_id.clone(),
            target_agent_id: None,
            expected_source_secret_env: None,
            label: "Replacement Agent".to_string(),
            icon: "terminal".to_string(),
            command_line: "replacement-agent".to_string(),
            command: "replacement-agent".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            secret_env: vec!["NEW_TOKEN".to_string()],
            enabled: true,
            confirmation: AgentReplaceCustomConfirmation {
                accepted_launch_identity_change: true,
            },
        })
        .unwrap();

    assert_eq!(replaced.old_agent_id, created.agent_id);
    assert_ne!(replaced.new_agent_id, replaced.old_agent_id);
    assert!(replaced.cleanup.removed_catalog_record);
    assert!(replaced.cleanup.removed_cached_status);
    assert!(!replaced.cleanup.removed_settings_overlay);
    assert_eq!(replaced.cleanup.removed_secret_env, ["OLD_TOKEN"]);
    assert_eq!(
        replaced.cleanup.history_policy,
        AgentReplaceCustomHistoryPolicy::PreserveHistoricalTasks
    );
    assert_eq!(
        statuses.snapshot(replaced.old_agent_id.as_str()).status,
        AgentStatus::Disconnected
    );

    let deleted = api
        .delete_custom(AgentDeleteCustomParams {
            agent_id: replaced.new_agent_id,
            expected_secret_env: None,
        })
        .unwrap();
    assert_eq!(deleted.removed_secret_env, ["NEW_TOKEN"]);
}

fn test_catalog_store() -> AgentCatalogStore {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.keep();
    let store = Store::open(path).unwrap();
    AgentCatalogStore::new(store)
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

struct ExplicitAuthAgent;

struct FailingAuthAgent;

impl AgentRuntime for FailingAuthAgent {
    fn authenticate(
        &self,
        _request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        Err(RuntimeError::Internal(
            "CODEX_API_KEY is not set: { secret vendor metadata }".to_string(),
        ))
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        unreachable!("authentication must not start a session")
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        unreachable!("authentication must not prompt")
    }
}

impl AgentRuntime for ExplicitAuthAgent {
    fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        Ok(AgentAuthenticateResult {
            agent_id: request.agent_id,
            method_id: request.method_id,
            status: crate::protocol::model::AgentAuthenticateStatus::Authenticated,
        })
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        unreachable!("authentication must not start a session")
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        unreachable!("authentication must not prompt")
    }
}

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

struct InternalFailingAgent;

impl AgentRuntime for InternalFailingAgent {
    fn probe(&self, _request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Err(RuntimeError::Internal("ACP connection failed".to_string()))
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
