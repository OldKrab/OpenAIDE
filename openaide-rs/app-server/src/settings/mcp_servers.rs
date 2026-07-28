use std::collections::BTreeSet;
use std::path::Path;

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::settings::{
    McpCreateServerParams, McpDeleteServerParams, McpGetServerDetailsParams,
    McpGetServerDetailsResult, McpMutationResult, McpServerConfiguration, McpServerDefinition,
    McpSetServerEnabledParams, McpUpdateServerParams, SettingsMcpServerRecord,
    SettingsMcpServerStatus, SettingsMcpServerTransport, SettingsMcpServersParams,
    SettingsMcpServersResult, SettingsProjectionAvailability,
};

use crate::protocol::errors::RuntimeError;
use crate::storage::Store;
use crate::time::now_string;

pub(crate) trait McpServersSettingsWorkflow: Send + Sync {
    fn mcp_servers_settings(
        &self,
        params: SettingsMcpServersParams,
    ) -> Result<SettingsMcpServersResult, ProtocolError>;
    fn mcp_server_details(
        &self,
        params: McpGetServerDetailsParams,
    ) -> Result<McpGetServerDetailsResult, ProtocolError>;
    fn create_mcp_server(
        &self,
        params: McpCreateServerParams,
    ) -> Result<McpMutationResult, ProtocolError>;
    fn update_mcp_server(
        &self,
        params: McpUpdateServerParams,
    ) -> Result<McpMutationResult, ProtocolError>;
    fn delete_mcp_server(
        &self,
        params: McpDeleteServerParams,
    ) -> Result<McpMutationResult, ProtocolError>;
    fn set_mcp_server_enabled(
        &self,
        params: McpSetServerEnabledParams,
    ) -> Result<McpMutationResult, ProtocolError>;
}

#[derive(Clone)]
pub(crate) struct McpServersSettingsService {
    store: Store,
}

impl McpServersSettingsService {
    pub(crate) fn new(store: Store) -> Self {
        Self { store }
    }

    fn current_settings(&self) -> Result<SettingsMcpServersResult, ProtocolError> {
        let servers = self
            .store
            .read_mcp_servers()
            .map_err(protocol_error_from_runtime)?
            .into_iter()
            .map(project_server)
            .collect();
        Ok(SettingsMcpServersResult {
            generated_at: now_string(),
            availability: SettingsProjectionAvailability::Available,
            servers,
            notices: Vec::new(),
        })
    }

    fn mutation_result(&self, server_id: String) -> Result<McpMutationResult, ProtocolError> {
        Ok(McpMutationResult {
            server_id,
            servers: self.current_settings()?,
        })
    }
}

impl McpServersSettingsWorkflow for McpServersSettingsService {
    fn mcp_servers_settings(
        &self,
        _params: SettingsMcpServersParams,
    ) -> Result<SettingsMcpServersResult, ProtocolError> {
        self.current_settings()
    }

    fn mcp_server_details(
        &self,
        params: McpGetServerDetailsParams,
    ) -> Result<McpGetServerDetailsResult, ProtocolError> {
        let server = self
            .store
            .read_mcp_servers()
            .map_err(protocol_error_from_runtime)?
            .into_iter()
            .find(|server| server.id == params.id)
            .ok_or_else(|| not_found(&params.id))?;
        Ok(McpGetServerDetailsResult {
            generated_at: now_string(),
            server,
        })
    }

    fn create_mcp_server(
        &self,
        params: McpCreateServerParams,
    ) -> Result<McpMutationResult, ProtocolError> {
        validate_server(&params.server)?;
        let server = self
            .store
            .create_mcp_server(params.server)
            .map_err(protocol_error_from_runtime)?;
        crate::logging::info(
            "mcp_server_created",
            serde_json::json!({ "serverId": &server.id }),
        );
        self.mutation_result(server.id)
    }

    fn update_mcp_server(
        &self,
        params: McpUpdateServerParams,
    ) -> Result<McpMutationResult, ProtocolError> {
        validate_server(&params.server)?;
        let server = self
            .store
            .update_mcp_server(params.server, &params.expected_secret_names)
            .map_err(protocol_error_from_runtime)?;
        crate::logging::info(
            "mcp_server_updated",
            serde_json::json!({ "serverId": &server.id }),
        );
        self.mutation_result(server.id)
    }

    fn delete_mcp_server(
        &self,
        params: McpDeleteServerParams,
    ) -> Result<McpMutationResult, ProtocolError> {
        self.store
            .delete_mcp_server(&params.id, &params.expected_secret_names)
            .map_err(protocol_error_from_runtime)?;
        crate::logging::info(
            "mcp_server_deleted",
            serde_json::json!({ "serverId": &params.id }),
        );
        self.mutation_result(params.id)
    }

    fn set_mcp_server_enabled(
        &self,
        params: McpSetServerEnabledParams,
    ) -> Result<McpMutationResult, ProtocolError> {
        let server = self
            .store
            .set_mcp_server_enabled(&params.id, params.enabled)
            .map_err(protocol_error_from_runtime)?;
        crate::logging::info(
            "mcp_server_availability_changed",
            serde_json::json!({ "serverId": &server.id, "enabled": server.enabled }),
        );
        self.mutation_result(server.id)
    }
}

fn project_server(server: McpServerDefinition) -> SettingsMcpServerRecord {
    let validation_error = validate_server(&server).err().map(|error| error.message);
    let status = if validation_error.is_some() {
        SettingsMcpServerStatus::Invalid
    } else if !server.enabled {
        SettingsMcpServerStatus::Disabled
    } else {
        SettingsMcpServerStatus::Configured
    };
    let transport = match server.configuration {
        McpServerConfiguration::Stdio { .. } => SettingsMcpServerTransport::Stdio,
        McpServerConfiguration::Http { .. } => SettingsMcpServerTransport::Http,
        McpServerConfiguration::Sse { .. } => SettingsMcpServerTransport::Sse,
    };
    SettingsMcpServerRecord {
        id: server.id,
        label: server.label,
        enabled: server.enabled,
        scope: server.scope,
        transport,
        status,
        description: server.description,
        validation_error,
    }
}

fn validate_server(server: &McpServerDefinition) -> Result<(), ProtocolError> {
    if server.id.is_empty()
        || server.id.len() > 128
        || !server
            .id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    {
        return Err(validation_error("MCP server id is invalid"));
    }
    if server.label.trim().is_empty() {
        return Err(validation_error("MCP server name is required"));
    }
    match &server.configuration {
        McpServerConfiguration::Stdio {
            command,
            env,
            secret_env,
            ..
        } => {
            if !Path::new(command).is_absolute() {
                return Err(validation_error(
                    "MCP stdio command must be an absolute path",
                ));
            }
            validate_secret_names(secret_env, env.keys().map(String::as_str))?;
        }
        McpServerConfiguration::Http {
            url,
            headers,
            secret_headers,
        }
        | McpServerConfiguration::Sse {
            url,
            headers,
            secret_headers,
        } => {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(validation_error("MCP server URL must use HTTP or HTTPS"));
            }
            validate_secret_names(secret_headers, headers.keys().map(String::as_str))?;
        }
    }
    Ok(())
}

fn validate_secret_names<'a>(
    secret_names: &[String],
    plain_names: impl Iterator<Item = &'a str>,
) -> Result<(), ProtocolError> {
    let plain_names = plain_names.collect::<BTreeSet<_>>();
    let mut unique = BTreeSet::new();
    for name in secret_names {
        if name.trim().is_empty() || !unique.insert(name.as_str()) {
            return Err(validation_error("MCP secret field names must be unique"));
        }
        if plain_names.contains(name.as_str()) {
            return Err(validation_error(
                "An MCP field cannot be both plain text and secret",
            ));
        }
    }
    Ok(())
}

fn not_found(id: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotFound,
        message: format!("MCP server not found: {id}"),
        recoverable: true,
        target: None,
    }
}

fn validation_error(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::ValidationFailed,
        message: message.to_string(),
        recoverable: false,
        target: None,
    }
}

fn protocol_error_from_runtime(error: RuntimeError) -> ProtocolError {
    let code = match error {
        RuntimeError::Conflict(_) => ProtocolErrorCode::Conflict,
        RuntimeError::InvalidParams(_) => ProtocolErrorCode::NotFound,
        _ => ProtocolErrorCode::Internal,
    };
    ProtocolError {
        code,
        message: error.to_string(),
        recoverable: true,
        target: None,
    }
}

#[cfg(test)]
#[path = "mcp_servers_tests.rs"]
mod tests;
