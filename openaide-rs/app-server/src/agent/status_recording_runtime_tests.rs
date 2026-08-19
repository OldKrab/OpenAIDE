use std::sync::Arc;

use crate::agent::catalog_store::AgentCatalogStore;
use crate::agent::mock::MockAgent;
use crate::agent::product_api::{AgentProductApi, AgentSettingsDetailsWorkflow};
use crate::agent::registry::AgentRegistry;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::runtime::AgentSessionStart;
use crate::agent::status_cache::AgentStatusCache;
use crate::agent::status_recording_runtime::AgentStatusRecordingRuntime;
use crate::agent::TurnCancellation;
use crate::storage::Store;
use openaide_app_server_protocol::agent::{AgentSettingsDetailsParams, AgentSettingsStatus};

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
