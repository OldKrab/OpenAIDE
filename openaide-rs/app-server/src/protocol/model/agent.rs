use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOptionsStatus {
    Ready,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

    pub fn model_id(&self) -> Option<String> {
        self.options
            .iter()
            .find(|option| matches!(option.category, Some(ConfigOptionCategory::Model)))
            .and_then(|option| option.current_value.as_id().map(ToOwned::to_owned))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOptionCategory {
    Mode,
    Model,
    ThoughtLevel,
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOptionKind {
    #[default]
    Select,
    Boolean,
}

/// A typed Agent-owned value. The discriminator prevents boolean values from
/// being confused with string IDs at storage and protocol boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigOptionCurrentValue {
    Id { value: String },
    Boolean { value: bool },
}

impl ConfigOptionCurrentValue {
    pub fn id(value: impl Into<String>) -> Self {
        Self::Id {
            value: value.into(),
        }
    }

    pub fn boolean(value: bool) -> Self {
        Self::Boolean { value }
    }

    pub fn as_id(&self) -> Option<&str> {
        match self {
            Self::Id { value } => Some(value),
            Self::Boolean { .. } => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Id { .. } => None,
            Self::Boolean { value } => Some(*value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ConfigOption {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<ConfigOptionCategory>,
    /// Legacy catalogs predate boolean options, so an omitted kind is always select.
    #[serde(default)]
    pub kind: ConfigOptionKind,
    #[serde(deserialize_with = "deserialize_current_value")]
    pub current_value: ConfigOptionCurrentValue,
    pub values: Vec<ConfigOptionValue>,
}

fn deserialize_current_value<'de, D>(deserializer: D) -> Result<ConfigOptionCurrentValue, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    if let Some(value) = value.as_str() {
        return Ok(ConfigOptionCurrentValue::id(value));
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentCommandsCatalog {
    pub commands: Vec<AgentCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentAuthMethodSummary {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub variables: Vec<AgentAuthVariableSummary>,
    pub link: Option<String>,
    pub terminal_args: Vec<String>,
    pub terminal_env: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentAuthVariableSummary {
    pub name: String,
    pub label: Option<String>,
    pub secret: bool,
    pub optional: bool,
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
    pub logout_supported: bool,
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
    AwaitingUser,
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
