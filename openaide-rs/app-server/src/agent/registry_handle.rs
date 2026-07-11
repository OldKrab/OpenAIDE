use std::sync::{Arc, RwLock};

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::registry::{AgentDefinitionSummary, AgentRegistry};
use crate::protocol::errors::RuntimeError;

#[derive(Debug, Clone)]
pub(crate) struct AgentRegistryHandle {
    inner: Arc<RwLock<AgentRegistry>>,
}

impl AgentRegistryHandle {
    pub(crate) fn new(registry: AgentRegistry) -> Self {
        Self {
            inner: Arc::new(RwLock::new(registry)),
        }
    }

    pub(crate) fn replace(&self, registry: AgentRegistry) {
        *self.inner.write().expect("Agent registry handle poisoned") = registry;
    }

    pub(crate) fn current(&self) -> AgentRegistry {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
            .clone()
    }

    pub(crate) fn require(&self, agent_id: &str) -> Result<(), RuntimeError> {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
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
            .display_name(agent_id, selected_label)
    }

    pub(crate) fn summaries(&self) -> Vec<AgentDefinitionSummary> {
        self.inner
            .read()
            .expect("Agent registry handle poisoned")
            .summaries()
    }
}

impl From<AgentRegistry> for AgentRegistryHandle {
    fn from(value: AgentRegistry) -> Self {
        Self::new(value)
    }
}
