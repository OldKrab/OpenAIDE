use super::*;
use crate::agent::events::{AgentNativeSubagentSpawned, AgentNativeSubagentStateUpdate};

#[derive(Default)]
pub(super) struct CaptureSink {
    pub(super) spawns: Mutex<Vec<AgentNativeSubagentSpawned>>,
    pub(super) updates: Mutex<Vec<AgentEvent>>,
    pub(super) details: Mutex<Vec<AgentNativeSubagentDetailsUpdate>>,
    pub(super) states: Mutex<Vec<AgentNativeSubagentState>>,
    pub(super) identities: Mutex<Vec<crate::agent::events::AgentNativeSubagentIdentityUpdate>>,
    /// What a root session projection emitted.
    pub(super) root_updates: Mutex<Vec<AgentEvent>>,
    /// The child the Task stores from an earlier connection.
    pub(super) stored: Mutex<Option<crate::agent::events::AgentNativeSubagentStoredIdentity>>,
}

impl AgentSessionEventSink for CaptureSink {
    fn session_update(&self, event: AgentEvent) -> Result<(), RuntimeError> {
        self.root_updates.lock().unwrap().push(event);
        Ok(())
    }

    fn subagent_session_update(
        &self,
        _session_id: &str,
        event: AgentEvent,
    ) -> Result<(), RuntimeError> {
        self.updates.lock().unwrap().push(event);
        Ok(())
    }

    fn config_options_changed(
        &self,
        _catalog: crate::protocol::model::ConfigOptionsCatalog,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn commands_changed(
        &self,
        _catalog: crate::protocol::model::AgentCommandsCatalog,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn stored_subagent_identity(
        &self,
        _native_session_id: &str,
    ) -> Result<Option<crate::agent::events::AgentNativeSubagentStoredIdentity>, RuntimeError> {
        Ok(self.stored.lock().unwrap().clone())
    }

    fn subagent_spawned(&self, event: AgentNativeSubagentSpawned) -> Result<(), RuntimeError> {
        self.spawns.lock().unwrap().push(event);
        Ok(())
    }

    fn subagent_state_changed(
        &self,
        event: AgentNativeSubagentStateUpdate,
    ) -> Result<(), RuntimeError> {
        self.states.lock().unwrap().push(event.state);
        Ok(())
    }

    fn subagent_details_changed(
        &self,
        event: AgentNativeSubagentDetailsUpdate,
    ) -> Result<(), RuntimeError> {
        self.details.lock().unwrap().push(event);
        Ok(())
    }

    fn subagent_identity_changed(
        &self,
        event: crate::agent::events::AgentNativeSubagentIdentityUpdate,
    ) -> Result<(), RuntimeError> {
        self.identities.lock().unwrap().push(event);
        Ok(())
    }
}

pub(super) fn claude_router(capture: &Arc<CaptureSink>) -> AcpNativeSubagentRouter {
    router_for(
        capture,
        "claude-code",
        "@agentclientprotocol/claude-agent-acp",
    )
}

pub(super) fn router_for(
    capture: &Arc<CaptureSink>,
    agent_id: &str,
    implementation_name: &str,
) -> AcpNativeSubagentRouter {
    let sinks: AcpSessionEventSinkMap = Arc::default();
    sinks.lock().unwrap().insert("root".into(), capture.clone());
    let router = AcpNativeSubagentRouter::new(agent_id, sinks);
    router.set_negotiated(true, Some(implementation_name));
    router
}

pub(super) fn claude_meta(
    subagent: serde_json::Value,
) -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({ "claudeCode": { "nativeSubagent": subagent } })
        .as_object()
        .unwrap()
        .clone()
}

pub(super) fn detail_pairs(details: &[AgentNativeSubagentDetail]) -> Vec<(&str, &str)> {
    details
        .iter()
        .map(|detail| (detail.label.as_str(), detail.value.as_str()))
        .collect()
}
