use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::client::SettingsSection;
use crate::settings::{AppPreferencesResult, RuntimeSettingsResult};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub sections: Vec<SettingsSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences: Option<AppPreferencesResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeSettingsResult>,
}
