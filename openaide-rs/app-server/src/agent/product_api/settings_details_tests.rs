use std::collections::HashMap;
use std::sync::Arc;

use openaide_app_server_protocol::agent::{
    AgentSettingsDetailsParams, AgentSettingsSourceKind, AgentSettingsStatus,
};
use openaide_app_server_protocol::snapshot::{AgentCapabilities, AgentStatus};

use crate::agent::catalog_store::AgentCatalogStore;
use crate::agent::product_api::{AgentProductApi, AgentSettingsDetailsWorkflow};
use crate::agent::registry::{AgentCatalogRecord, AgentRegistry, CODEX_AGENT_ID};
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::runtime::{
    AgentEventSink, AgentProbeRequest, AgentPrompt, AgentRuntime, AgentSession, AgentSessionStart,
};
use crate::agent::status_cache::{AgentStatusCache, AgentStatusSnapshot};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentAuthMethodSummary, AgentAuthVariableSummary, AgentProbeCapabilities, AgentProbeResult,
    AgentProbeStatus,
};
use crate::storage::Store;

#[test]
fn agent_settings_details_include_disabled_builtins_and_custom_launch_details() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let catalog_store = AgentCatalogStore::new(store);
    catalog_store
        .save_records(&[
            AgentCatalogRecord::disabled_builtin(CODEX_AGENT_ID.to_string()),
            AgentCatalogRecord::custom(
                "custom.local".to_string(),
                "Local Agent".to_string(),
                "terminal".to_string(),
                true,
                "local-agent".to_string(),
                "local-agent \"--flag with spaces\"".to_string(),
                vec!["--flag with spaces".to_string()],
                HashMap::from([("LOCAL_FLAG".to_string(), "1".to_string())]),
                vec!["LOCAL_TOKEN".to_string()],
            ),
        ])
        .unwrap();
    let statuses = AgentStatusCache::default();
    statuses.record_for_test(
        "custom.local".to_string(),
        AgentStatusSnapshot {
            status: AgentStatus::Connected,
            setup_reason: None,
            capabilities: AgentCapabilities::default(),
            auth_methods: Vec::new(),
            logout_supported: false,
            authenticating_method_id: None,
            status_before_authentication: None,
        },
    );
    let api = AgentProductApi::new(
        AgentRegistryHandle::new(AgentRegistry::default_built_ins()),
        catalog_store,
        Arc::new(ProbeReadyAgentRuntime),
        statuses,
    );

    let result = api
        .agent_settings_details(AgentSettingsDetailsParams {})
        .unwrap();

    let codex = result
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == CODEX_AGENT_ID)
        .unwrap();
    assert!(!codex.enabled);
    assert_eq!(codex.status, AgentSettingsStatus::Disabled);

    let custom = result
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == "custom.local")
        .unwrap();
    assert_eq!(custom.source_kind, AgentSettingsSourceKind::Custom);
    assert_eq!(custom.icon, "terminal");
    assert_eq!(
        custom.command_line.as_deref(),
        Some("local-agent \"--flag with spaces\"")
    );
    assert_eq!(custom.status, AgentSettingsStatus::Connected);
    assert_eq!(custom.env.len(), 2);

    let opencode = result
        .agents
        .iter()
        .find(|agent| agent.agent_id.as_str() == "opencode")
        .unwrap();
    assert_eq!(opencode.status, AgentSettingsStatus::Connected);
    assert!(opencode.logout_supported);
    assert_eq!(opencode.auth_methods.len(), 1);
    let method = &opencode.auth_methods[0];
    assert_eq!(method.id, "api-key");
    assert_eq!(method.kind, "env_var");
    assert_eq!(method.link.as_deref(), Some("https://example.com/keys"));
    assert_eq!(method.variables.len(), 2);
    assert_eq!(method.variables[0].name, "API_KEY");
    assert!(method.variables[0].secret);
    assert!(!method.variables[0].optional);
    assert_eq!(method.variables[1].label.as_deref(), Some("Endpoint"));
    assert!(!method.variables[1].secret);
}

struct ProbeReadyAgentRuntime;

impl AgentRuntime for ProbeReadyAgentRuntime {
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Ok(AgentProbeResult {
            agent_id: request.agent_id,
            status: AgentProbeStatus::Ready,
            protocol_version: "1".to_string(),
            implementation_name: None,
            implementation_version: None,
            capabilities: Vec::new(),
            typed_capabilities: AgentProbeCapabilities::default(),
            auth_methods: vec![AgentAuthMethodSummary {
                id: "api-key".to_string(),
                label: "API key".to_string(),
                kind: "env_var".to_string(),
                description: Some("Authenticate with environment variables".to_string()),
                variables: vec![
                    AgentAuthVariableSummary {
                        name: "API_KEY".to_string(),
                        label: None,
                        secret: true,
                        optional: false,
                    },
                    AgentAuthVariableSummary {
                        name: "ENDPOINT".to_string(),
                        label: Some("Endpoint".to_string()),
                        secret: false,
                        optional: true,
                    },
                ],
                link: Some("https://example.com/keys".to_string()),
                terminal_args: Vec::new(),
                terminal_env: HashMap::new(),
            }],
            logout_supported: true,
        })
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        unreachable!("settings details must not start agent sessions")
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        unreachable!("settings details must not prompt agents")
    }
}
