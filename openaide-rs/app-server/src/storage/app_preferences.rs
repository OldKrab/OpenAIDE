use openaide_app_server_protocol::settings::{
    AppPreferences, AppPreferencesPatch, ComposerSubmitShortcut,
};
use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;

use super::{atomic, Store};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAppPreferences {
    #[serde(default = "default_submit_shortcut")]
    composer_submit_shortcut: ComposerSubmitShortcut,
}

impl From<StoredAppPreferences> for AppPreferences {
    fn from(value: StoredAppPreferences) -> Self {
        Self {
            composer_submit_shortcut: value.composer_submit_shortcut,
        }
    }
}

impl From<AppPreferences> for StoredAppPreferences {
    fn from(value: AppPreferences) -> Self {
        Self {
            composer_submit_shortcut: value.composer_submit_shortcut,
        }
    }
}

impl Store {
    pub fn read_app_preferences(&self) -> Result<AppPreferences, RuntimeError> {
        let path = self.app_preferences_path();
        if !path.exists() {
            return Ok(AppPreferences::default());
        }
        let text = std::fs::read_to_string(path)?;
        let stored: StoredAppPreferences = serde_json::from_str(&text)?;
        Ok(stored.into())
    }

    pub fn update_app_preferences(
        &self,
        patch: AppPreferencesPatch,
    ) -> Result<AppPreferences, RuntimeError> {
        let mut preferences = self.read_app_preferences()?;
        preferences.composer_submit_shortcut = patch.composer_submit_shortcut;
        self.write_app_preferences(&preferences)?;
        Ok(preferences)
    }

    fn write_app_preferences(&self, preferences: &AppPreferences) -> Result<(), RuntimeError> {
        atomic::write_json(
            &self.app_preferences_path(),
            &StoredAppPreferences::from(preferences.clone()),
        )
    }

    fn app_preferences_path(&self) -> std::path::PathBuf {
        self.settings_dir().join("app_preferences.json")
    }
}

fn default_submit_shortcut() -> ComposerSubmitShortcut {
    ComposerSubmitShortcut::ModEnter
}

#[cfg(test)]
#[path = "app_preferences_tests.rs"]
mod tests;
