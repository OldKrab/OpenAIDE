use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::ProjectId;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettingsParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferencesParams {}

/// Updates the state-root-wide Project used as the fallback for a generic New Task.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NewTaskDefaultsUpdateParams {
    pub project_id: ProjectId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMcpServersParams {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpGetServerDetailsParams {
    /// Stable opaque identifier returned by `settings/getMcpServers`.
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpCreateServerParams {
    pub server: McpServerDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpUpdateServerParams {
    pub server: McpServerDefinition,
    /// Guards secure-storage cleanup against a stale editor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_secret_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpDeleteServerParams {
    pub id: String,
    /// Names the host can safely remove after the durable definition is deleted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_secret_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpSetServerEnabledParams {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSkillsParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResetTaskHistoryParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResetTaskHistoryResult {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSkillDetailsParams {
    /// Opaque identifier returned by `settings/getSkills`.
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMcpServersResult {
    pub generated_at: String,
    pub availability: SettingsProjectionAvailability,
    pub servers: Vec<SettingsMcpServerRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<SettingsProjectionNotice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpGetServerDetailsResult {
    pub generated_at: String,
    pub server: McpServerDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpMutationResult {
    pub server_id: String,
    pub servers: SettingsMcpServersResult,
}

/// Durable MCP configuration. Secret fields contain names only; their values
/// remain in shell-owned secure storage keyed by this definition's stable id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDefinition {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    pub scope: McpServerScope,
    pub configuration: McpServerConfiguration,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpServerScope {
    Global,
    Project { project_id: ProjectId },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "transport",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpServerConfiguration {
    Stdio {
        /// Original user input retained so the editor can preserve formatting.
        command_line: String,
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        secret_env: Vec<String>,
    },
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        secret_headers: Vec<String>,
    },
    /// SSE remains editable for compatibility but is deprecated by ACP.
    Sse {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        secret_headers: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSkillsResult {
    pub generated_at: String,
    pub availability: SettingsProjectionAvailability,
    pub skills: Vec<SettingsSkillRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<SettingsProjectionNotice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSkillDetailsResult {
    pub generated_at: String,
    pub skill: SettingsSkillRecord,
    pub document: SettingsSkillDocument,
}

/// Parsed `SKILL.md` content. Required fields stay first-class while unknown
/// frontmatter remains visible without making the protocol agent-specific.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSkillDocument {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_fields: Vec<SettingsSkillDocumentField>,
    pub instructions: String,
    /// Exact file contents for lossless inspection and unsupported YAML syntax.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSkillDocumentField {
    pub name: String,
    /// YAML representation of the field value, including nested structures.
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SettingsProjectionAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsProjectionNotice {
    pub severity: SettingsProjectionNoticeSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SettingsProjectionNoticeSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SettingsScope {
    Global,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMcpServerRecord {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub scope: McpServerScope,
    pub transport: SettingsMcpServerTransport,
    pub status: SettingsMcpServerStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SettingsMcpServerTransport {
    Stdio,
    Http,
    Sse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SettingsMcpServerStatus {
    Configured,
    Invalid,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSkillRecord {
    pub id: String,
    pub label: String,
    pub scope: SettingsScope,
    pub source_label: String,
    pub status: SettingsSkillStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub last_scanned_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SettingsSkillStatus {
    Valid,
    Warning,
    Invalid,
    Shadowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferencesUpdateParams {
    pub preferences: AppPreferencesPatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferencesPatch {
    pub composer_submit_shortcut: ComposerSubmitShortcut,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferencesResult {
    pub preferences: AppPreferences,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferences {
    pub composer_submit_shortcut: ComposerSubmitShortcut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ComposerSubmitShortcut {
    ModEnter,
    Enter,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            composer_submit_shortcut: ComposerSubmitShortcut::Enter,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettingsUpdateParams {
    pub developer: RuntimeDeveloperSettingsPatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDeveloperSettingsPatch {
    pub acp_trace: RuntimeAcpTraceSettingsPatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAcpTraceSettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettingsResult {
    pub developer: RuntimeDeveloperSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDeveloperSettings {
    pub acp_trace: RuntimeAcpTraceSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAcpTraceSettings {
    pub enabled: bool,
    pub directory: String,
}
