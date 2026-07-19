use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::AgentId;
use openaide_app_server_protocol::snapshot::{AgentCollectionSnapshot, AgentSummary};

use crate::agent::registry::AgentDefinitionSummary;
#[cfg(test)]
use crate::agent::registry::AgentRegistry;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::status_cache::AgentStatusCache;

pub trait AgentCollectionSnapshotSource: Send + Sync {
    fn snapshot(&self) -> Result<AgentCollectionSnapshot, ProtocolError>;
}

#[derive(Debug, Clone)]
pub struct AgentRegistrySnapshotSource {
    registry: AgentRegistryHandle,
    statuses: AgentStatusCache,
}

impl AgentRegistrySnapshotSource {
    #[cfg(test)]
    pub(crate) fn new(registry: AgentRegistry) -> Self {
        Self::with_status_cache(
            AgentRegistryHandle::new(registry),
            AgentStatusCache::default(),
        )
    }

    pub(crate) fn with_status_cache(
        registry: AgentRegistryHandle,
        statuses: AgentStatusCache,
    ) -> Self {
        Self { registry, statuses }
    }
}

impl AgentCollectionSnapshotSource for AgentRegistrySnapshotSource {
    fn snapshot(&self) -> Result<AgentCollectionSnapshot, ProtocolError> {
        Ok(collection_from_registry_summaries_with_statuses(
            self.registry.summaries(),
            &self.statuses,
        ))
    }
}

#[cfg(test)]
fn collection_from_registry_summaries(
    summaries: Vec<AgentDefinitionSummary>,
) -> AgentCollectionSnapshot {
    collection_from_registry_summaries_with_statuses(summaries, &AgentStatusCache::default())
}

fn collection_from_registry_summaries_with_statuses(
    summaries: Vec<AgentDefinitionSummary>,
    statuses: &AgentStatusCache,
) -> AgentCollectionSnapshot {
    AgentCollectionSnapshot {
        agents: summaries
            .into_iter()
            .map(|agent| {
                let status = statuses.snapshot(&agent.id);
                AgentSummary {
                    agent_id: AgentId::from(agent.id),
                    label: agent.label,
                    status: status.status,
                    setup_reason: status.setup_reason,
                    capabilities: status.capabilities,
                }
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "agent_collection_tests.rs"]
mod tests;
