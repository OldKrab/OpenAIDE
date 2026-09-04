use super::*;
use crate::agent::registry::AgentRegistry;
use crate::agent::{AgentEventSink, AgentPrompt, AgentRuntime, AgentSession, AgentSessionStart};
use crate::protocol::model::{AgentProbeCapabilities, AgentProbeStatus, IsolationKind, TaskStatus};
use crate::storage::records::{
    TaskPreparationRecord, TaskRecord, TaskTitle, TaskTitleSource, TaskTitleState,
};
use crate::storage::Store;
use openaide_app_server_protocol::agent::{
    AgentAuthenticateParams, AgentAuthenticateStatus as ProtocolAgentAuthenticateStatus,
    AgentCreateCustomParams, AgentDeleteCustomParams, AgentLogoutParams,
    AgentReplaceCustomConfirmation, AgentReplaceCustomHistoryPolicy, AgentReplaceCustomParams,
};
use openaide_app_server_protocol::ids::AgentId;
use openaide_app_server_protocol::snapshot::AgentSignInPhase;
use openaide_app_server_protocol::snapshot::AgentStatus;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
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
fn internal_probe_failure_returns_failed_agent_status_for_explicit_retry() {
    let statuses = AgentStatusCache::default();
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(InternalFailingAgent),
        statuses.clone(),
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
    assert_eq!(codex.status, AgentStatus::Failed);
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
fn successful_authentication_persists_cleanup_provenance_for_settings() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        AgentCatalogStore::new(store.clone()),
        Arc::new(ExplicitAuthAgent),
        AgentStatusCache::default(),
    );
    api.authenticate(AgentAuthenticateParams {
        agent_id: AgentId::from("codex"),
        method_id: "api-key".to_string(),
        env: BTreeMap::new(),
        secret_env: Vec::new(),
        secret_storage_agent_id: None,
        terminal_confirmed: false,
    })
    .unwrap();

    let reopened = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        AgentCatalogStore::new(store),
        Arc::new(ReadyAgent),
        AgentStatusCache::default(),
    );
    let details = reopened
        .agent_settings_details(openaide_app_server_protocol::agent::AgentSettingsDetailsParams {})
        .unwrap();
    let codex = details
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == "codex")
        .unwrap();

    assert_eq!(
        codex.last_authentication_method_id.as_deref(),
        Some("api-key")
    );
}

#[test]
fn logout_calls_the_agent_and_marks_authentication_required() {
    let statuses = AgentStatusCache::default();
    statuses.record_probe_success(&AgentProbeResult {
        agent_id: "codex".to_string(),
        status: AgentProbeStatus::Ready,
        protocol_version: "1".to_string(),
        implementation_name: None,
        implementation_version: None,
        capabilities: Vec::new(),
        typed_capabilities: AgentProbeCapabilities::default(),
        auth_methods: Vec::new(),
        logout_supported: true,
    });
    let logout_count = Arc::new(AtomicUsize::new(0));
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(LogoutAgent(logout_count.clone())),
        statuses.clone(),
    );
    api.authenticate(AgentAuthenticateParams {
        agent_id: AgentId::from("codex"),
        method_id: "api-key".to_string(),
        env: BTreeMap::new(),
        secret_env: Vec::new(),
        secret_storage_agent_id: None,
        terminal_confirmed: false,
    })
    .unwrap();

    let result = api
        .logout(AgentLogoutParams {
            agent_id: AgentId::from("codex"),
            expected_method_id: Some("api-key".to_string()),
        })
        .unwrap();

    assert_eq!(logout_count.load(Ordering::SeqCst), 1);
    assert_eq!(statuses.snapshot("codex").status, AgentStatus::AuthRequired);
    assert!(result.agents.agents.iter().any(|agent| {
        agent.agent_id.as_str() == "codex" && agent.status == AgentStatus::AuthRequired
    }));
    let details = api
        .agent_settings_details(openaide_app_server_protocol::agent::AgentSettingsDetailsParams {})
        .unwrap();
    let codex = details
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == "codex")
        .unwrap();
    assert_eq!(codex.last_authentication_method_id, None);
}

#[test]
fn logout_is_blocked_while_the_agent_has_a_running_task() {
    let statuses = AgentStatusCache::default();
    statuses.record_probe_success(&AgentProbeResult {
        agent_id: "codex".to_string(),
        status: AgentProbeStatus::Ready,
        protocol_version: "1".to_string(),
        implementation_name: None,
        implementation_version: None,
        capabilities: Vec::new(),
        typed_capabilities: AgentProbeCapabilities::default(),
        auth_methods: Vec::new(),
        logout_supported: true,
    });
    let catalog_store = test_catalog_store();
    catalog_store
        .backing_store()
        .write_task(&running_task_record("task-running"))
        .unwrap();
    let logout_count = Arc::new(AtomicUsize::new(0));
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        catalog_store,
        Arc::new(LogoutAgent(logout_count.clone())),
        statuses,
    );
    api.authenticate(AgentAuthenticateParams {
        agent_id: AgentId::from("codex"),
        method_id: "api-key".to_string(),
        env: BTreeMap::new(),
        secret_env: Vec::new(),
        secret_storage_agent_id: None,
        terminal_confirmed: false,
    })
    .unwrap();

    let details = api
        .agent_settings_details(openaide_app_server_protocol::agent::AgentSettingsDetailsParams {})
        .unwrap();
    assert!(
        details
            .agents
            .iter()
            .find(|agent| agent.agent_id.as_str() == "codex")
            .unwrap()
            .logout_blocked_by_running_task
    );
    let error = api
        .logout(AgentLogoutParams {
            agent_id: AgentId::from("codex"),
            expected_method_id: Some("api-key".to_string()),
        })
        .unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::Conflict);
    assert_eq!(logout_count.load(Ordering::SeqCst), 0);
}

#[test]
fn cancelling_authentication_restores_the_prior_status() {
    let statuses = AgentStatusCache::default();
    statuses.record_probe_error(
        "codex",
        &RuntimeError::AuthRequired("Authentication required".to_string()),
    );
    let api = AgentProductApi::new(
        AgentRegistry::default_built_ins(),
        test_catalog_store(),
        Arc::new(ReadyAgent),
        statuses.clone(),
    );
    statuses
        .begin_authentication("codex", "chat-gpt", false)
        .expect("begin authentication");

    let result = api
        .cancel_authenticate(
            openaide_app_server_protocol::agent::AgentCancelAuthenticateParams {
                agent_id: AgentId::from("codex"),
            },
        )
        .unwrap();

    assert_eq!(statuses.snapshot("codex").status, AgentStatus::AuthRequired);
    assert!(result.agents.agents.iter().any(
        |agent| agent.agent_id.as_str() == "codex" && agent.status == AgentStatus::AuthRequired
    ));
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
        "Codex could not sign in with api-key. Try again or choose another method."
    );
    assert!(!error.message.contains("CODEX_API_KEY"));
    let snapshot = statuses.snapshot("codex");
    assert_eq!(snapshot.status, AgentStatus::Disconnected);
    // The failure stays on the Sign-in Flow so every client renders it, still without Agent text.
    let flow = snapshot.sign_in.expect("failed flow is kept for the user");
    assert_eq!(flow.phase, AgentSignInPhase::Failed);
    assert_eq!(flow.method_id, "api-key");
    assert_eq!(flow.failure.as_deref(), Some(error.message.as_str()));
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

fn running_task_record(task_id: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: TaskTitleState::from_title(TaskTitle::new(task_id, TaskTitleSource::User)),
        status: TaskStatus::Active,
        task_version: 1,
        message_history_version: 1,
        unread: false,
        pinned: false,
        attention: None,
        created_at: "1".to_string(),
        updated_at: "1".to_string(),
        last_activity: "1".to_string(),
        permission_policy: Default::default(),
        composer_history: Default::default(),
        message_queue: Default::default(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Open,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        tombstoned: false,
        revision: 1,
        config_options_catalog: None,
        native_session_data_freshness: Default::default(),
        native_session_reload_requirement: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        context_usage: None,
        current_plan: None,
        completed_plan_message_id: None,
        last_turn_usage: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
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
                fork_sessions: false,
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

struct LogoutAgent(Arc<AtomicUsize>);

impl AgentRuntime for LogoutAgent {
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

    fn logout(&self, _agent_id: &str) -> Result<(), RuntimeError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        unreachable!("logout must not start a session")
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        unreachable!("logout must not prompt")
    }
}

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
