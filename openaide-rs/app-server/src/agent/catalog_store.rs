use serde::{Deserialize, Serialize};

use crate::agent::registry::{AgentCatalogRecord, AgentRegistry};
use crate::protocol::errors::RuntimeError;
use crate::storage::{atomic, Store};

const CATALOG_SCHEMA_VERSION: u32 = 1;
const CATALOG_FILE_NAME: &str = "catalog.json";

#[derive(Clone)]
pub(crate) struct AgentCatalogStore {
    store: Store,
}

impl AgentCatalogStore {
    pub(crate) fn new(store: Store) -> Self {
        Self { store }
    }

    pub(crate) fn registry(&self) -> Result<AgentRegistry, RuntimeError> {
        AgentRegistry::from_catalog_overlay(self.load_records()?)
    }

    pub(crate) fn load_records(&self) -> Result<Vec<AgentCatalogRecord>, RuntimeError> {
        let path = self.catalog_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let catalog: StoredAgentCatalog = serde_json::from_slice(&std::fs::read(path)?)?;
        if catalog.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(RuntimeError::Storage(format!(
                "unsupported agent catalog schema {}",
                catalog.schema_version
            )));
        }
        Ok(catalog.records)
    }

    pub(crate) fn save_records(&self, records: &[AgentCatalogRecord]) -> Result<(), RuntimeError> {
        atomic::write_json(
            &self.catalog_path(),
            &StoredAgentCatalog {
                schema_version: CATALOG_SCHEMA_VERSION,
                records: records.to_vec(),
            },
        )
    }

    pub(crate) fn save_custom(
        &self,
        record: AgentCatalogRecord,
    ) -> Result<AgentRegistry, RuntimeError> {
        if !record.is_custom() {
            return Err(RuntimeError::InvalidParams("agent.source_kind".to_string()));
        }
        let id = record.id()?;
        let mut records = self.load_records()?;
        if let Some(existing) = records
            .iter_mut()
            .find(|item| item.id().map(|item_id| item_id == id).unwrap_or(false))
        {
            if !existing.is_custom() {
                return Err(RuntimeError::InvalidParams("agent.source_kind".to_string()));
            }
            *existing = record;
        } else {
            records.push(record);
        }
        let registry = AgentRegistry::from_catalog_overlay(records.clone())?;
        reject_duplicate_custom_setup(&records, &id)?;
        self.save_records(&records)?;
        Ok(registry)
    }

    pub(crate) fn update_custom_metadata(
        &self,
        agent_id: &str,
        label: String,
        icon: String,
        enabled: bool,
    ) -> Result<AgentRegistry, RuntimeError> {
        let mut records = self.load_records()?;
        let record = records
            .iter_mut()
            .find(|item| {
                item.id()
                    .map(|item_id| item_id == agent_id)
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                RuntimeError::CapabilityMissing(format!("custom agent {agent_id} is not available"))
            })?;
        if !record.is_custom() {
            return Err(RuntimeError::InvalidParams("agent.source_kind".to_string()));
        }
        record.set_metadata(label, icon, enabled);
        let registry = AgentRegistry::from_catalog_overlay(records.clone())?;
        reject_duplicate_custom_setup(&records, agent_id)?;
        self.save_records(&records)?;
        Ok(registry)
    }

    pub(crate) fn replace_custom(
        &self,
        source_agent_id: &str,
        replacement: AgentCatalogRecord,
    ) -> Result<AgentRegistry, RuntimeError> {
        if !replacement.is_custom() {
            return Err(RuntimeError::InvalidParams("agent.source_kind".to_string()));
        }
        let replacement_id = replacement.id()?;
        let mut records = self.load_records()?;
        let source = records
            .iter()
            .find(|item| {
                item.id()
                    .map(|item_id| item_id == source_agent_id)
                    .unwrap_or(false)
                    && item.is_custom()
            })
            .ok_or_else(|| {
                RuntimeError::CapabilityMissing(format!(
                    "custom agent {source_agent_id} is not available"
                ))
            })?;
        if source.same_launch_identity(&replacement) {
            return Err(RuntimeError::InvalidParams("agent.launch".to_string()));
        }
        let before = records.len();
        records.retain(|item| {
            item.id()
                .map(|item_id| item_id != source_agent_id || !item.is_custom())
                .unwrap_or(true)
        });
        if records.len() == before {
            return Err(RuntimeError::CapabilityMissing(format!(
                "custom agent {source_agent_id} is not available"
            )));
        }
        records.push(replacement);
        let registry = AgentRegistry::from_catalog_overlay(records.clone())?;
        reject_duplicate_custom_setup(&records, &replacement_id)?;
        self.save_records(&records)?;
        Ok(registry)
    }

    pub(crate) fn delete_custom(&self, agent_id: &str) -> Result<AgentRegistry, RuntimeError> {
        let mut records = self.load_records()?;
        let before = records.len();
        records.retain(|item| {
            item.id()
                .map(|item_id| item_id != agent_id || !item.is_custom())
                .unwrap_or(true)
        });
        if records.len() == before {
            return Err(RuntimeError::CapabilityMissing(format!(
                "custom agent {agent_id} is not available"
            )));
        }
        self.save_and_registry(records)
    }

    pub(crate) fn set_enabled(
        &self,
        agent_id: &str,
        enabled: bool,
    ) -> Result<AgentRegistry, RuntimeError> {
        let mut records = self.load_records()?;
        if let Some(record) = records.iter_mut().find(|item| {
            item.id()
                .map(|item_id| item_id == agent_id)
                .unwrap_or(false)
        }) {
            record.set_enabled(enabled);
            if !record.is_custom() && enabled {
                records.retain(|item| item.id().map(|item_id| item_id != agent_id).unwrap_or(true));
            }
            return self.save_and_registry(records);
        }
        if enabled {
            AgentRegistry::default_built_ins().require(agent_id)?;
            return self.save_and_registry(records);
        }
        AgentRegistry::default_built_ins().require(agent_id)?;
        records.push(AgentCatalogRecord::disabled_builtin(agent_id.to_string()));
        self.save_and_registry(records)
    }

    fn save_and_registry(
        &self,
        records: Vec<AgentCatalogRecord>,
    ) -> Result<AgentRegistry, RuntimeError> {
        let registry = AgentRegistry::from_catalog_overlay(records.clone())?;
        self.save_records(&records)?;
        Ok(registry)
    }

    fn catalog_path(&self) -> std::path::PathBuf {
        self.store.agents_dir().join(CATALOG_FILE_NAME)
    }
}

fn reject_duplicate_custom_setup(
    records: &[AgentCatalogRecord],
    agent_id: &str,
) -> Result<(), RuntimeError> {
    let candidate = records
        .iter()
        .find(|item| {
            item.id()
                .map(|item_id| item_id == agent_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| RuntimeError::InvalidParams("agent.id".to_string()))?;
    let label_key = candidate.normalized_label_key();
    let launch_key = candidate.normalized_launch_command_key();
    AgentRegistry::default_built_ins().reject_duplicate_setup_keys(
        agent_id,
        &label_key,
        &launch_key,
    )?;
    for record in records {
        if !record.is_custom() {
            continue;
        }
        let id = record.id()?;
        if id == agent_id {
            continue;
        }
        if record.normalized_label_key() == label_key {
            return Err(RuntimeError::InvalidParams("agent.label".to_string()));
        }
        if record.normalized_launch_command_key() == launch_key {
            return Err(RuntimeError::InvalidParams("agent.command".to_string()));
        }
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct StoredAgentCatalog {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    records: Vec<AgentCatalogRecord>,
}

#[cfg(test)]
mod tests;
