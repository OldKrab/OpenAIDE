use openaide_app_server_protocol::client::SettingsSection;
use openaide_app_server_protocol::settings::{
    AppPreferences, AppPreferencesParams, AppPreferencesResult, AppPreferencesUpdateParams,
    ComposerSubmitShortcut, RuntimeAcpTraceSettings, RuntimeDeveloperSettings,
    RuntimeSettingsResult,
};
use std::sync::Arc;

use super::{
    AppPreferencesWorkflow, RuntimeSettingsWorkflow, SettingsCatalog, SettingsSnapshotSource,
};

#[test]
fn product_defaults_expose_renderable_settings_sections() {
    let snapshot = SettingsCatalog::default().snapshot(None).unwrap();

    assert_eq!(
        snapshot.sections,
        vec![SettingsSection::Agents, SettingsSection::CommonSettings]
    );
    assert_eq!(snapshot.preferences, None);
    assert_eq!(snapshot.runtime, None);
}

#[test]
fn section_filter_returns_only_requested_section() {
    let snapshot = SettingsCatalog::default()
        .snapshot(Some(SettingsSection::Agents))
        .unwrap();

    assert_eq!(snapshot.sections, vec![SettingsSection::Agents]);
}

#[test]
fn section_filter_omits_sections_without_backend_projection() {
    let snapshot = SettingsCatalog::default()
        .snapshot(Some(SettingsSection::McpServers))
        .unwrap();

    assert_eq!(snapshot.sections, Vec::<SettingsSection>::new());
}

#[test]
fn backend_settings_expose_non_agent_projection_sections() {
    let snapshot = SettingsCatalog::with_backend_settings(
        Arc::new(FixedAppPreferences),
        Arc::new(FixedRuntimeSettings),
    )
    .snapshot(None)
    .unwrap();

    assert_eq!(
        snapshot.sections,
        vec![
            SettingsSection::Agents,
            SettingsSection::McpServers,
            SettingsSection::Skills,
            SettingsSection::CommonSettings
        ]
    );
}

#[test]
fn section_filter_returns_mcp_section_when_backend_projection_is_available() {
    let snapshot = SettingsCatalog::with_backend_settings(
        Arc::new(FixedAppPreferences),
        Arc::new(FixedRuntimeSettings),
    )
    .snapshot(Some(SettingsSection::McpServers))
    .unwrap();

    assert_eq!(snapshot.sections, vec![SettingsSection::McpServers]);
}

#[test]
fn snapshot_includes_backend_settings_when_available() {
    let snapshot = SettingsCatalog::with_backend_settings(
        Arc::new(FixedAppPreferences),
        Arc::new(FixedRuntimeSettings),
    )
    .snapshot(None)
    .unwrap();

    assert_eq!(
        snapshot.preferences,
        Some(AppPreferencesResult {
            preferences: AppPreferences {
                composer_submit_shortcut: ComposerSubmitShortcut::Enter,
            },
        })
    );
    assert_eq!(
        snapshot.runtime,
        Some(RuntimeSettingsResult {
            developer: RuntimeDeveloperSettings {
                acp_trace: RuntimeAcpTraceSettings {
                    enabled: true,
                    directory: "/runtime/acp-traces".to_string(),
                },
            },
        })
    );
}

#[derive(Debug)]
struct FixedAppPreferences;

impl AppPreferencesWorkflow for FixedAppPreferences {
    fn app_preferences(
        &self,
        _params: AppPreferencesParams,
    ) -> Result<AppPreferencesResult, openaide_app_server_protocol::errors::ProtocolError> {
        Ok(AppPreferencesResult {
            preferences: AppPreferences {
                composer_submit_shortcut: ComposerSubmitShortcut::Enter,
            },
        })
    }

    fn update_app_preferences(
        &self,
        _params: AppPreferencesUpdateParams,
    ) -> Result<AppPreferencesResult, openaide_app_server_protocol::errors::ProtocolError> {
        self.app_preferences(AppPreferencesParams {})
    }
}

#[derive(Debug)]
struct FixedRuntimeSettings;

impl RuntimeSettingsWorkflow for FixedRuntimeSettings {
    fn runtime_settings(
        &self,
    ) -> Result<RuntimeSettingsResult, openaide_app_server_protocol::errors::ProtocolError> {
        Ok(RuntimeSettingsResult {
            developer: RuntimeDeveloperSettings {
                acp_trace: RuntimeAcpTraceSettings {
                    enabled: true,
                    directory: "/runtime/acp-traces".to_string(),
                },
            },
        })
    }

    fn update_runtime_settings(
        &self,
        _params: openaide_app_server_protocol::settings::RuntimeSettingsUpdateParams,
    ) -> Result<RuntimeSettingsResult, openaide_app_server_protocol::errors::ProtocolError> {
        self.runtime_settings()
    }
}
