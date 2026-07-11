use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::settings::{
    SettingsMcpServersParams, SettingsMcpServersResult, SettingsProjectionAvailability,
};

use crate::time::now_string;

pub(crate) trait McpServersSettingsWorkflow: Send + Sync {
    fn mcp_servers_settings(
        &self,
        params: SettingsMcpServersParams,
    ) -> Result<SettingsMcpServersResult, ProtocolError>;
}

#[derive(Debug, Clone, Default)]
pub(crate) struct McpServersSettingsService;

impl McpServersSettingsService {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl McpServersSettingsWorkflow for McpServersSettingsService {
    fn mcp_servers_settings(
        &self,
        _params: SettingsMcpServersParams,
    ) -> Result<SettingsMcpServersResult, ProtocolError> {
        Ok(SettingsMcpServersResult {
            generated_at: now_string(),
            availability: SettingsProjectionAvailability::Unavailable,
            servers: Vec::new(),
            notices: Vec::new(),
        })
    }
}
