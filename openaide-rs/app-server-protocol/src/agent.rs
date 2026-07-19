use serde::{Deserialize, Serialize};
use ts_rs::TS;

use std::collections::BTreeMap;

use crate::ids::{AgentId, ProjectId};
use crate::snapshot::AgentCollectionSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentProbeParams {
    pub agent_id: AgentId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentProbeResult {
    pub agents: AgentCollectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuthenticateParams {
    pub agent_id: AgentId,
    pub method_id: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_env: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_storage_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub terminal_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuthenticateResult {
    pub agent_id: AgentId,
    pub method_id: String,
    pub status: AgentAuthenticateStatus,
    pub agents: AgentCollectionSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentAuthenticateStatus {
    Authenticated,
    AwaitingUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentListSessionsParams {
    pub agent_id: AgentId,
    pub project_id: ProjectId,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentListedSession {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentListSessionsResult {
    pub agent_id: AgentId,
    pub project_id: ProjectId,
    pub project_label: String,
    pub sessions: Vec<AgentListedSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentCreateCustomParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
    pub label: String,
    #[serde(default = "default_custom_icon")]
    pub icon: String,
    #[serde(default)]
    pub command_line: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub secret_env: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentCreateCustomResult {
    pub agent_id: AgentId,
    pub agents: AgentCollectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdateCustomMetadataParams {
    pub agent_id: AgentId,
    pub label: String,
    #[serde(default = "default_custom_icon")]
    pub icon: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdateCustomMetadataResult {
    pub agent_id: AgentId,
    pub agents: AgentCollectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentReplaceCustomParams {
    pub source_agent_id: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<AgentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_secret_env: Option<Vec<String>>,
    pub label: String,
    #[serde(default = "default_custom_icon")]
    pub icon: String,
    #[serde(default)]
    pub command_line: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_env: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub confirmation: AgentReplaceCustomConfirmation,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentReplaceCustomConfirmation {
    pub accepted_launch_identity_change: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentReplaceCustomResult {
    pub old_agent_id: AgentId,
    pub new_agent_id: AgentId,
    pub cleanup: AgentReplaceCustomCleanup,
    pub agents: AgentCollectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentReplaceCustomCleanup {
    pub removed_catalog_record: bool,
    pub removed_cached_status: bool,
    pub removed_settings_overlay: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_secret_env: Vec<String>,
    pub history_policy: AgentReplaceCustomHistoryPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentReplaceCustomHistoryPolicy {
    PreserveHistoricalTasks,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentDeleteCustomParams {
    pub agent_id: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_secret_env: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentDeleteCustomResult {
    pub agent_id: AgentId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_secret_env: Vec<String>,
    pub agents: AgentCollectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSetEnabledParams {
    pub agent_id: AgentId,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSetEnabledResult {
    pub agents: AgentCollectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsDetailsParams {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsDetailsResult {
    pub generated_at: String,
    pub agents: Vec<AgentSettingsDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsDetail {
    pub agent_id: AgentId,
    pub label: String,
    pub enabled: bool,
    pub source_kind: AgentSettingsSourceKind,
    pub icon: String,
    pub transport: AgentSettingsTransport,
    pub status: AgentSettingsStatus,
    pub launch_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_line: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<AgentSettingsEnvRow>,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth_methods: Vec<AgentSettingsAuthMethod>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub logout_supported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authenticating_method_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentSettingsSourceKind {
    BuiltIn,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentSettingsTransport {
    Stdio,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentSettingsStatus {
    Disconnected,
    Launching,
    Connected,
    SetupRequired,
    AuthRequired,
    Authenticating,
    Unsupported,
    Failed,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsEnvRow {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsAuthMethod {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<AgentSettingsAuthVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminal_args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub terminal_env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsAuthVariable {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub secret: bool,
    pub optional: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_custom_icon() -> String {
    "bot".to_string()
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
