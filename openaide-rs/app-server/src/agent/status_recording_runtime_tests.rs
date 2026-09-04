use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::agent::catalog_store::AgentCatalogStore;
use crate::agent::mock::MockAgent;
use crate::agent::product_api::{AgentProductApi, AgentSettingsDetailsWorkflow};
use crate::agent::registry::AgentRegistry;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::runtime::{
    AgentEventSink, AgentListSessionsRequest, AgentPrompt, AgentRuntime, AgentSession,
    AgentSessionStart,
};
use crate::agent::status_cache::AgentStatusCache;
use crate::agent::status_recording_runtime::AgentStatusRecordingRuntime;
use crate::agent::{AgentProbeRequest, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentAuthMethodSummary, AgentProbeCapabilities, AgentProbeResult, AgentProbeStatus,
};
use crate::storage::Store;
use openaide_app_server_protocol::agent::{AgentSettingsDetailsParams, AgentSettingsStatus};

#[test]
fn logout_reaches_the_wrapped_agent_runtime() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = AgentStatusRecordingRuntime::wrap(
        Arc::new(LogoutRuntime(calls.clone())),
        AgentStatusCache::default(),
    );

    runtime.logout("custom.test").unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn start_session_marks_enabled_agent_connected_in_settings_details() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let statuses = AgentStatusCache::default();
    let runtime = AgentStatusRecordingRuntime::wrap(Arc::new(MockAgent), statuses.clone());
    runtime
        .start_session(AgentSessionStart {
            agent_id: "codex".to_string(),
            task_id: "task-1".to_string(),
            cwd: "/tmp".to_string(),
            model_id: None,
            context: Vec::new(),
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .unwrap();

    let api = AgentProductApi::new(
        AgentRegistryHandle::new(AgentRegistry::default_built_ins()),
        AgentCatalogStore::new(store),
        runtime,
        statuses,
    );
    let result = api
        .agent_settings_details(AgentSettingsDetailsParams {})
        .unwrap();
    let codex = result
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == "codex")
        .unwrap();
    assert_eq!(codex.status, AgentSettingsStatus::Connected);
}

#[test]
fn list_sessions_auth_required_leaves_launching_and_keeps_advertised_methods() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let statuses = AgentStatusCache::default();
    statuses.record_launching("codex");
    let runtime =
        AgentStatusRecordingRuntime::wrap(Arc::new(AuthRequiredListRuntime), statuses.clone());

    let error = runtime
        .list_sessions(AgentListSessionsRequest {
            agent_id: "codex".to_string(),
            cwd: Some("/tmp".to_string()),
            cursor: None,
        })
        .unwrap_err();
    assert!(matches!(
        error,
        crate::protocol::errors::RuntimeError::AuthRequired(_)
    ));

    let api = AgentProductApi::new(
        AgentRegistryHandle::new(AgentRegistry::default_built_ins()),
        AgentCatalogStore::new(store),
        runtime,
        statuses,
    );
    let result = api
        .agent_settings_details(AgentSettingsDetailsParams {})
        .unwrap();
    let codex = result
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == "codex")
        .unwrap();
    assert_eq!(codex.status, AgentSettingsStatus::AuthRequired);
    assert_eq!(
        codex
            .auth_methods
            .iter()
            .map(|method| method.label.as_str())
            .collect::<Vec<_>>(),
        ["ChatGPT"]
    );
}

struct AuthRequiredListRuntime;

struct LogoutRuntime(Arc<AtomicUsize>);

impl AgentRuntime for LogoutRuntime {
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

impl AgentRuntime for AuthRequiredListRuntime {
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Ok(AgentProbeResult {
            agent_id: request.agent_id,
            status: AgentProbeStatus::Ready,
            protocol_version: "test".to_string(),
            implementation_name: Some("Codex".to_string()),
            implementation_version: None,
            capabilities: Vec::new(),
            typed_capabilities: AgentProbeCapabilities::default(),
            auth_methods: vec![AgentAuthMethodSummary {
                id: "chatgpt".to_string(),
                label: "ChatGPT".to_string(),
                kind: "agent".to_string(),
                description: None,
                variables: Vec::new(),
                link: None,
                terminal_args: Vec::new(),
                terminal_env: Default::default(),
            }],
            logout_supported: false,
        })
    }

    fn list_sessions(
        &self,
        _request: AgentListSessionsRequest,
    ) -> Result<crate::protocol::model::AgentListSessionsResult, RuntimeError> {
        Err(RuntimeError::AuthRequired(
            "Authentication required".to_string(),
        ))
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        unreachable!("list-session status recording must not start sessions")
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        unreachable!("list-session status recording must not prompt")
    }
}
