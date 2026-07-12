use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::snapshot::{AgentCapabilities, AgentStatus};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::AgentProbeResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentStatusSnapshot {
    pub(crate) status: AgentStatus,
    pub(crate) capabilities: AgentCapabilities,
}

impl Default for AgentStatusSnapshot {
    fn default() -> Self {
        Self {
            status: AgentStatus::Disconnected,
            capabilities: AgentCapabilities::default(),
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
            },
        );
    }

    pub(crate) fn record_probe_error(&self, agent_id: &str, error: &RuntimeError) {
        self.record(
            agent_id.to_string(),
            AgentStatusSnapshot {
                status: status_from_probe_error(error),
                capabilities: AgentCapabilities::default(),
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
        | RuntimeError::Storage(_) => AgentStatus::Failed,
    }
}

#[cfg(test)]
#[path = "status_cache_tests.rs"]
mod tests;
