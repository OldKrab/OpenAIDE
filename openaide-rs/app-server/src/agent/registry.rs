use std::collections::HashMap;

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::registry_builtin;
use crate::agent::registry_catalog;
use crate::protocol::errors::RuntimeError;
use crate::protocol::params::TaskCreateParams;

pub(crate) use crate::agent::registry_catalog::AgentCatalogRecord;

pub(crate) const CODEX_AGENT_ID: &str = "codex";
pub(crate) const CODEX_AGENT_LABEL: &str = "Codex";
pub(crate) const OPENCODE_AGENT_ID: &str = "opencode";
pub(crate) const OPENCODE_AGENT_LABEL: &str = "OpenCode";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentSourceKind {
    BuiltIn,
    Custom,
}

#[derive(Debug, Clone)]
pub(crate) enum AgentLaunch {
    AcpStdio(AcpAgentConfig),
}

#[derive(Debug, Clone)]
pub(crate) struct AgentDefinition {
    id: String,
    label: String,
    source_kind: AgentSourceKind,
    launch: AgentLaunch,
}

impl AgentDefinition {
    pub(super) fn new(
        id: String,
        label: String,
        source_kind: AgentSourceKind,
        launch: AgentLaunch,
    ) -> Self {
        Self {
            id,
            label,
            source_kind,
            launch,
        }
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn acp_stdio_config(&self) -> AcpAgentConfig {
        match (&self.launch, self.source_kind) {
            (AgentLaunch::AcpStdio(config), AgentSourceKind::BuiltIn) => config.clone(),
            (AgentLaunch::AcpStdio(config), AgentSourceKind::Custom) => config.clone(),
        }
    }

    pub(crate) fn options_request_key(&self, cwd: &str) -> String {
        format!("{}\0{cwd}", self.id)
    }

    fn normalized_label_key(&self) -> String {
        registry_catalog::normalized_label_key(&self.label)
    }

    fn normalized_launch_command_key(&self) -> String {
        match &self.launch {
            AgentLaunch::AcpStdio(config) => normalized_launch_command_key(
                std::iter::once(config.command.as_str())
                    .chain(config.args.iter().map(String::as_str)),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AgentRegistry {
    agents: HashMap<String, AgentDefinition>,
}

impl AgentRegistry {
    pub(crate) fn codex(config: AcpAgentConfig) -> Self {
        let codex = registry_builtin::codex_definition(config);
        Self {
            agents: HashMap::from([(codex.id.clone(), codex)]),
        }
    }

    pub(crate) fn built_ins() -> Self {
        Self::from_definitions(registry_builtin::default_definitions())
    }

    pub(crate) fn default_built_ins() -> Self {
        Self::built_ins()
    }

    #[cfg(test)]
    pub(crate) fn from_agent_catalog(
        records: Vec<AgentCatalogRecord>,
    ) -> Result<Self, RuntimeError> {
        let mut agents = HashMap::new();
        for record in records {
            let Some(definition) = registry_catalog::definition_from_record(record)? else {
                continue;
            };
            if agents.insert(definition.id.clone(), definition).is_some() {
                return Err(RuntimeError::InvalidParams("agents.id".to_string()));
            }
        }
        if agents.is_empty() {
            return Err(RuntimeError::InvalidParams("agents".to_string()));
        }
        Ok(Self { agents })
    }

    pub(crate) fn from_catalog_overlay(
        records: Vec<AgentCatalogRecord>,
    ) -> Result<Self, RuntimeError> {
        let mut registry = Self::default_built_ins();
        let mut seen = std::collections::HashSet::new();
        for record in records {
            let id = registry_catalog::record_id(&record)?;
            if !seen.insert(id.clone()) {
                return Err(RuntimeError::InvalidParams("agents.id".to_string()));
            }
            if !registry_catalog::record_enabled(&record) {
                registry.agents.remove(&id);
                continue;
            }
            let Some(definition) = registry_catalog::definition_from_record(record)? else {
                continue;
            };
            registry.agents.insert(definition.id.clone(), definition);
        }
        Ok(registry)
    }

    pub(crate) fn require(&self, agent_id: &str) -> Result<&AgentDefinition, RuntimeError> {
        self.agents.get(agent_id).ok_or_else(|| {
            RuntimeError::CapabilityMissing(format!("agent {agent_id} is not available"))
        })
    }

    pub(crate) fn require_acp_config(
        &self,
        agent_id: &str,
    ) -> Result<AcpAgentConfig, RuntimeError> {
        Ok(self.require(agent_id)?.acp_stdio_config())
    }

    pub(crate) fn display_name(
        &self,
        agent_id: &str,
        selected_label: Option<&str>,
    ) -> Result<String, RuntimeError> {
        if let Some(label) = selected_label
            .map(str::trim)
            .filter(|label| !label.is_empty())
        {
            return Ok(label.chars().take(80).collect());
        }
        Ok(self.require(agent_id)?.label().to_string())
    }

    pub(crate) fn validate_task_create(
        &self,
        params: &TaskCreateParams,
    ) -> Result<(), RuntimeError> {
        self.require(&params.selected_agent_id)?;
        if params.model_id.is_some() {
            return Err(RuntimeError::CapabilityMissing(
                "agent_config_options".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn summaries(&self) -> Vec<AgentDefinitionSummary> {
        let mut summaries: Vec<_> = self
            .agents
            .values()
            .map(|agent| AgentDefinitionSummary {
                id: agent.id().to_string(),
                label: agent.label().to_string(),
                source_kind: agent.source_kind,
            })
            .collect();
        summaries.sort_by(|left, right| left.id.cmp(&right.id));
        summaries
    }

    pub(crate) fn reject_duplicate_setup_keys(
        &self,
        agent_id: &str,
        label_key: &str,
        launch_key: &str,
    ) -> Result<(), RuntimeError> {
        for agent in self.agents.values() {
            if agent.id() == agent_id {
                continue;
            }
            if agent.normalized_label_key() == label_key {
                return Err(RuntimeError::InvalidParams("agent.label".to_string()));
            }
            if agent.normalized_launch_command_key() == launch_key {
                return Err(RuntimeError::InvalidParams("agent.command".to_string()));
            }
        }
        Ok(())
    }

    fn from_definitions(definitions: impl IntoIterator<Item = AgentDefinition>) -> Self {
        Self {
            agents: definitions
                .into_iter()
                .map(|definition| (definition.id.clone(), definition))
                .collect(),
        }
    }
}

fn normalized_launch_command_key<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    registry_catalog::normalized_launch_command_key(parts)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentDefinitionSummary {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) source_kind: AgentSourceKind,
}

#[cfg(test)]
mod tests;
