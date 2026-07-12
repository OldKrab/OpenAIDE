use openaide_app_server_protocol::ids::AgentId;
use uuid::Uuid;

use crate::agent::registry::{AgentDefinitionSummary, CODEX_AGENT_ID};

const CUSTOM_AGENT_PREFIX: &str = "custom.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentIdentityValidationError {
    field: &'static str,
}

impl AgentIdentityValidationError {
    pub(crate) fn field(self) -> &'static str {
        self.field
    }
}

pub(crate) fn generated_custom_agent_id() -> AgentId {
    AgentId::from(format!("{CUSTOM_AGENT_PREFIX}{}", Uuid::new_v4()))
}

pub(crate) fn normalized_existing_custom_agent_id(
    agent_id: AgentId,
) -> Result<AgentId, AgentIdentityValidationError> {
    let id = agent_id.into_string();
    if id.starts_with(CUSTOM_AGENT_PREFIX) && valid_agent_id(&id) {
        return Ok(AgentId::from(id));
    }
    Err(validation_error("agentId"))
}

pub(crate) fn normalized_label(label: String) -> Result<String, AgentIdentityValidationError> {
    let label = label.trim();
    if label.is_empty() {
        return Err(validation_error("label"));
    }
    Ok(label.chars().take(80).collect())
}

pub(crate) fn normalized_icon(icon: String) -> String {
    icon.trim().chars().take(40).collect()
}

pub(crate) fn valid_env_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|item| item == '_' || item.is_ascii_alphanumeric())
}

pub(crate) fn default_agent_id(summaries: &[AgentDefinitionSummary]) -> Option<AgentId> {
    summaries
        .iter()
        .find(|agent| agent.id == CODEX_AGENT_ID)
        .or_else(|| summaries.first())
        .map(|agent| AgentId::from(agent.id.clone()))
}

fn valid_agent_id(value: &str) -> bool {
    !value.trim().is_empty()
        && value
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || matches!(item, '_' | '-' | '.'))
}

fn validation_error(field: &'static str) -> AgentIdentityValidationError {
    AgentIdentityValidationError { field }
}

#[cfg(test)]
#[path = "agent_identity_tests.rs"]
mod tests;
