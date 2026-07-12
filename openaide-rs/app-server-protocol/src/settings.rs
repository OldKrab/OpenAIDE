use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettingsParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferencesParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMcpServersParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSkillsParams {}

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
pub struct SettingsSkillsResult {
    pub generated_at: String,
    pub availability: SettingsProjectionAvailability,
    pub skills: Vec<SettingsSkillRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<SettingsProjectionNotice>,
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
    pub scope: SettingsScope,
    pub transport: SettingsMcpServerTransport,
    pub status: SettingsMcpServerStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_summary: Option<String>,
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
    Unknown,
    Available,
    Failed,
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
