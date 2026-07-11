use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOptionsStatus {
    Ready,
    Empty,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigOptionsCatalog {
    pub agent_id: String,
    pub status: ConfigOptionsStatus,
    pub options: Vec<ConfigOption>,
}

impl ConfigOptionsCatalog {
    pub fn empty(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            status: ConfigOptionsStatus::Empty,
            options: Vec::new(),
        }
    }

    pub fn current_values(&self) -> HashMap<String, String> {
        self.options
            .iter()
            .map(|option| (option.id.clone(), option.current_value.clone()))
            .collect()
    }

    pub fn model_id(&self) -> Option<String> {
        self.options
            .iter()
            .find(|option| matches!(option.category, Some(ConfigOptionCategory::Model)))
            .map(|option| option.current_value.clone())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOptionCategory {
    Mode,
    Model,
    ThoughtLevel,
    Other,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigOption {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<ConfigOptionCategory>,
    pub current_value: String,
    pub values: Vec<ConfigOptionValue>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigOptionValue {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_label: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentCommandsCatalog {
    pub commands: Vec<AgentCommand>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentCommand {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProbeStatus {
    Ready,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentAuthMethodSummary {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentProbeResult {
    pub agent_id: String,
    pub status: AgentProbeStatus,
    pub protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implementation_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implementation_version: Option<String>,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub typed_capabilities: AgentProbeCapabilities,
    pub auth_methods: Vec<AgentAuthMethodSummary>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentProbeCapabilities {
    pub resume_sessions: bool,
    pub delete_sessions: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAuthenticateStatus {
    Authenticated,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentAuthenticateResult {
    pub agent_id: String,
    pub method_id: String,
    pub status: AgentAuthenticateStatus,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentListedSession {
    pub session_id: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentListSessionsResult {
    pub agent_id: String,
    pub sessions: Vec<AgentListedSession>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
