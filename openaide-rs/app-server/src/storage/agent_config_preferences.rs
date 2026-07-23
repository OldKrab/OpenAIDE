use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ConfigOptionCurrentValue, ConfigOptionsCatalog};

use super::{atomic, Store};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
struct StoredAgentConfigPreferences {
    #[serde(default)]
    agents: BTreeMap<String, AgentConfigPreferences>,
}

/// Durable user-selected values for one Agent. ACP remains authoritative for
/// labels, available choices, and the current state of every Native Session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct AgentConfigPreferences {
    #[serde(default)]
    pub(crate) options: Vec<AgentConfigPreference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct AgentConfigPreference {
    pub(crate) id: String,
    pub(crate) value: ConfigOptionCurrentValue,
}

impl Store {
    pub(crate) fn read_agent_config_preferences(
        &self,
        agent_id: &str,
    ) -> Result<AgentConfigPreferences, RuntimeError> {
        Ok(self
            .read_all_agent_config_preferences()?
            .agents
            .remove(agent_id)
            .unwrap_or_default())
    }

    /// Replaces one Agent's preference overlay from a complete confirmed ACP catalog.
    pub(crate) fn write_agent_config_preferences(
        &self,
        catalog: &ConfigOptionsCatalog,
    ) -> Result<bool, RuntimeError> {
        let _guard = self.lock_settings_write();
        let mut stored = self.read_all_agent_config_preferences()?;
        let preferences = AgentConfigPreferences {
            options: catalog
                .options
                .iter()
                .map(|option| AgentConfigPreference {
                    id: option.id.clone(),
                    value: option.current_value.clone(),
                })
                .collect(),
        };
        if stored.agents.get(&catalog.agent_id) == Some(&preferences) {
            return Ok(false);
        }
        stored.agents.insert(catalog.agent_id.clone(), preferences);
        atomic::write_json(&self.agent_config_preferences_path(), &stored)?;
        Ok(true)
    }

    fn read_all_agent_config_preferences(
        &self,
    ) -> Result<StoredAgentConfigPreferences, RuntimeError> {
        let path = self.agent_config_preferences_path();
        if !path.exists() {
            return Ok(StoredAgentConfigPreferences::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    fn agent_config_preferences_path(&self) -> std::path::PathBuf {
        self.settings_dir().join("agent_config_preferences.json")
    }
}

#[cfg(test)]
#[path = "agent_config_preferences_tests.rs"]
mod tests;
