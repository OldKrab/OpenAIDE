use openaide_app_server_protocol::client::SettingsSection;
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::settings::{AppPreferencesResult, RuntimeSettingsResult};
use openaide_app_server_protocol::snapshot::SettingsSnapshot;
use std::sync::Arc;

mod app_preferences;
mod mcp_servers;
mod runtime_settings;
mod skills;
pub(crate) use app_preferences::{AppPreferencesService, AppPreferencesWorkflow};
pub(crate) use mcp_servers::{McpServersSettingsService, McpServersSettingsWorkflow};
pub(crate) use runtime_settings::{RuntimeSettingsService, RuntimeSettingsWorkflow};
pub(crate) use skills::{SkillsSettingsService, SkillsSettingsWorkflow};

pub trait SettingsSnapshotSource: Send + Sync {
    fn snapshot(&self, section: Option<SettingsSection>)
        -> Result<SettingsSnapshot, ProtocolError>;
}

#[derive(Clone)]
pub struct SettingsCatalog {
    sections: Vec<SettingsSection>,
    app_preferences: Option<Arc<dyn AppPreferencesWorkflow>>,
    runtime_settings: Option<Arc<dyn RuntimeSettingsWorkflow>>,
}

impl SettingsCatalog {
    pub fn product_defaults() -> Self {
        Self {
            sections: vec![SettingsSection::Agents, SettingsSection::CommonSettings],
            app_preferences: None,
            runtime_settings: None,
        }
    }

    pub(crate) fn with_backend_settings(
        app_preferences: Arc<dyn AppPreferencesWorkflow>,
        runtime_settings: Arc<dyn RuntimeSettingsWorkflow>,
    ) -> Self {
        Self {
            sections: vec![
                SettingsSection::Agents,
                SettingsSection::McpServers,
                SettingsSection::Skills,
                SettingsSection::CommonSettings,
            ],
            app_preferences: Some(app_preferences),
            runtime_settings: Some(runtime_settings),
        }
    }
}

impl Default for SettingsCatalog {
    fn default() -> Self {
        Self::product_defaults()
    }
}

impl SettingsSnapshotSource for SettingsCatalog {
    fn snapshot(
        &self,
        section: Option<SettingsSection>,
    ) -> Result<SettingsSnapshot, ProtocolError> {
        let sections = match section {
            Some(section) if self.sections.contains(&section) => vec![section],
            Some(_) => Vec::new(),
            None => self.sections.clone(),
        };
        Ok(SettingsSnapshot {
            sections,
            preferences: self.app_preferences()?,
            runtime: self.runtime_settings()?,
        })
    }
}

impl SettingsCatalog {
    fn app_preferences(&self) -> Result<Option<AppPreferencesResult>, ProtocolError> {
        self.app_preferences
            .as_ref()
            .map(|app_preferences| app_preferences.app_preferences(Default::default()))
            .transpose()
    }

    fn runtime_settings(&self) -> Result<Option<RuntimeSettingsResult>, ProtocolError> {
        self.runtime_settings
            .as_ref()
            .map(|runtime_settings| runtime_settings.runtime_settings())
            .transpose()
    }
}

#[cfg(test)]
mod tests;
