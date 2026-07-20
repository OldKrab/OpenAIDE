use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::ProtocolError;
use crate::ids::{AgentConfigOptionId, ClientMutationId};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAgentConfigSnapshot {
    pub state: LiveSessionDataState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<AgentConfigOptionSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_change: Option<PendingAgentConfigChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigOptionSnapshot {
    pub config_id: AgentConfigOptionId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub kind: AgentConfigOptionKind,
    pub current_value: AgentConfigOptionCurrentValue,
    pub values: Vec<AgentConfigOptionValueSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentConfigOptionKind {
    Select,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentConfigOptionCurrentValue {
    Id { value: String },
    Boolean { value: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigOptionValueSnapshot {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PendingAgentConfigChange {
    pub client_mutation_id: ClientMutationId,
    pub config_id: AgentConfigOptionId,
    pub requested_value: AgentConfigOptionCurrentValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAgentCommandsSnapshot {
    pub state: LiveSessionDataState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<AgentSlashCommandSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSlashCommandSnapshot {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<AgentSlashCommandInputSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSlashCommandInputSnapshot {
    pub hint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LiveSessionDataState {
    Loading,
    Ready,
    Stale,
    Unavailable,
    Failed,
}
