use std::sync::{Arc, RwLock};

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::registry::{AgentDefinitionSummary, AgentRegistry};
use crate::protocol::errors::RuntimeError;

#[derive(Debug, Clone)]
pub(crate) struct AgentRegistryHandle {
    inner: Arc<RwLock<RegistryState>>,
}

#[derive(Debug)]
struct RegistryState {
    registry: AgentRegistry,
    revision: u64,
}

impl AgentRegistryHandle {
    pub(crate) fn new(registry: AgentRegistry) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RegistryState {
                registry,
                revision: 0,
            })),
        }
    }

    pub(crate) fn replace(&self, registry: AgentRegistry) {
        let mut state = self.inner.write().expect("Agent registry handle poisoned");
        state.registry = registry;
        state.revision += 1;
    }

    pub(crate) fn revision(&self) -> u64 {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
            .revision
    }

    /// Pins the Agent scope only across a local commit, never across Agent I/O.
    pub(crate) fn with_revision<T>(
        &self,
        revision: u64,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        let state = self.inner.read().expect("Agent registry handle poisoned");
        (state.revision == revision).then(operation)
    }

    pub(crate) fn current(&self) -> AgentRegistry {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
            .registry
            .clone()
    }

    pub(crate) fn require(&self, agent_id: &str) -> Result<(), RuntimeError> {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
            .registry
            .require(agent_id)
            .map(|_| ())
    }

    pub(crate) fn require_acp_config(
        &self,
        agent_id: &str,
    ) -> Result<AcpAgentConfig, RuntimeError> {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
            .registry
            .require_acp_config(agent_id)
    }

    pub(crate) fn display_name(
        &self,
        agent_id: &str,
        selected_label: Option<&str>,
    ) -> Result<String, RuntimeError> {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
            .registry
            .display_name(agent_id, selected_label)
    }

    pub(crate) fn summaries(&self) -> Vec<AgentDefinitionSummary> {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
            .registry
            .summaries()
    }
}

impl From<AgentRegistry> for AgentRegistryHandle {
    fn from(value: AgentRegistry) -> Self {
        Self::new(value)
    }
}
