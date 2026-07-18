use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::snapshot::{AgentCapabilities, AgentStatus};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentAuthMethodSummary, AgentProbeResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentStatusSnapshot {
    pub(crate) status: AgentStatus,
    pub(crate) capabilities: AgentCapabilities,
    pub(crate) auth_methods: Vec<AgentAuthMethodSummary>,
    pub(crate) logout_supported: bool,
    pub(crate) authenticating_method_id: Option<String>,
    pub(crate) status_before_authentication: Option<AgentStatus>,
}

impl Default for AgentStatusSnapshot {
    fn default() -> Self {
        Self {
            status: AgentStatus::Disconnected,
            capabilities: AgentCapabilities::default(),
            auth_methods: Vec::new(),
            logout_supported: false,
            authenticating_method_id: None,
            status_before_authentication: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AgentStatusCache {
    entries: Arc<Mutex<HashMap<String, AgentStatusSnapshot>>>,
}

impl AgentStatusCache {
    pub(crate) fn record_probe_success(&self, result: &AgentProbeResult) {
        self.record(
            result.agent_id.clone(),
            AgentStatusSnapshot {
                status: AgentStatus::Connected,
                capabilities: capabilities_from_probe(result),
                auth_methods: result.auth_methods.clone(),
                logout_supported: result.logout_supported,
                authenticating_method_id: None,
                status_before_authentication: None,
            },
        );
    }

    pub(crate) fn record_probe_error(&self, agent_id: &str, error: &RuntimeError) {
        self.record(
            agent_id.to_string(),
            AgentStatusSnapshot {
                status: status_from_probe_error(error),
                capabilities: AgentCapabilities::default(),
                auth_methods: Vec::new(),
                logout_supported: false,
                authenticating_method_id: None,
                status_before_authentication: None,
            },
        );
    }

    pub(crate) fn snapshot(&self, agent_id: &str) -> AgentStatusSnapshot {
        self.entries
            .lock()
            .expect("agent status cache poisoned")
            .get(agent_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn begin_authentication(
        &self,
        agent_id: &str,
        method_id: &str,
        continuing: bool,
    ) -> Result<(), RuntimeError> {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        if snapshot.status == AgentStatus::Authenticating {
            if continuing && snapshot.authenticating_method_id.as_deref() == Some(method_id) {
                return Ok(());
            }
            return Err(RuntimeError::Conflict(format!(
                "Authentication is already in progress for Agent {agent_id}"
            )));
        }
        snapshot.status_before_authentication = Some(snapshot.status.clone());
        snapshot.status = AgentStatus::Authenticating;
        snapshot.authenticating_method_id = Some(method_id.to_string());
        Ok(())
    }

    pub(crate) fn record_authentication_success(&self, agent_id: &str) {
        self.update_authentication_result(agent_id, AgentStatus::Connected);
    }

    pub(crate) fn record_authentication_error(&self, agent_id: &str, error: &RuntimeError) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        snapshot.status = snapshot
            .status_before_authentication
            .take()
            .unwrap_or_else(|| status_from_probe_error(error));
        snapshot.authenticating_method_id = None;
    }

    pub(crate) fn clear(&self, agent_id: &str) -> bool {
        self.entries
            .lock()
            .expect("agent status cache poisoned")
            .remove(agent_id)
            .is_some()
    }

    fn record(&self, agent_id: String, snapshot: AgentStatusSnapshot) {
        self.entries
            .lock()
            .expect("agent status cache poisoned")
            .insert(agent_id, snapshot);
    }

    fn update_authentication_result(&self, agent_id: &str, status: AgentStatus) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        snapshot.status = status;
        snapshot.authenticating_method_id = None;
        snapshot.status_before_authentication = None;
    }

    #[cfg(test)]
    pub(crate) fn record_for_test(&self, agent_id: String, snapshot: AgentStatusSnapshot) {
        self.record(agent_id, snapshot);
    }
}

fn capabilities_from_probe(result: &AgentProbeResult) -> AgentCapabilities {
    AgentCapabilities {
        resume_tasks: result.typed_capabilities.resume_sessions,
        delete_native_sessions: result.typed_capabilities.delete_sessions,
    }
}

fn status_from_probe_error(error: &RuntimeError) -> AgentStatus {
    match error {
        RuntimeError::AuthRequired(_) => AgentStatus::AuthRequired,
        RuntimeError::SetupRequired(_) => AgentStatus::SetupRequired,
        RuntimeError::Unsupported(_) => AgentStatus::Unsupported,
        RuntimeError::CapabilityMissing(_) | RuntimeError::MethodNotFound(_) => {
            AgentStatus::Unsupported
        }
        RuntimeError::NotReady(_)
        | RuntimeError::Internal(_)
        | RuntimeError::InvalidParams(_)
        | RuntimeError::TaskNotFound(_)
        | RuntimeError::Storage(_)
        | RuntimeError::Conflict(_) => AgentStatus::Failed,
    }
}

#[cfg(test)]
#[path = "status_cache_tests.rs"]
mod tests;
