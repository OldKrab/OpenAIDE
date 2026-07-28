use std::collections::BTreeMap;

use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::settings::McpServerScope;
use openaide_app_server_protocol::settings::{McpServerConfiguration, McpServerDefinition};
use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;

use super::{atomic, Store};

const MCP_SERVERS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredMcpServers {
    #[serde(default = "mcp_servers_schema_version")]
    version: u32,
    #[serde(default)]
    servers: Vec<McpServerDefinition>,
}

impl Store {
    /// Reads durable definitions only. Secret values are owned by the shell and
    /// are resolved later when an ACP session request is prepared.
    pub fn read_mcp_servers(&self) -> Result<Vec<McpServerDefinition>, RuntimeError> {
        Ok(self.read_mcp_server_catalog()?.servers)
    }

    /// Resolves the definitions visible to a new session. Project definitions
    /// with the same user-facing name replace global definitions.
    pub fn effective_mcp_servers(
        &self,
        project_id: Option<&ProjectId>,
    ) -> Result<Vec<McpServerDefinition>, RuntimeError> {
        let servers = self.read_mcp_servers()?;
        let mut effective = BTreeMap::<String, McpServerDefinition>::new();
        for server in servers
            .iter()
            .filter(|server| server.enabled && matches!(server.scope, McpServerScope::Global))
        {
            effective.insert(server_name_key(&server.label), server.clone());
        }
        if let Some(project_id) = project_id {
            for server in servers.iter().filter(|server| {
                server.enabled
                    && matches!(
                        &server.scope,
                        McpServerScope::Project {
                            project_id: configured
                        } if configured == project_id
                    )
            }) {
                effective.insert(server_name_key(&server.label), server.clone());
            }
        }
        Ok(effective.into_values().collect())
    }

    pub fn create_mcp_server(
        &self,
        server: McpServerDefinition,
    ) -> Result<McpServerDefinition, RuntimeError> {
        let _guard = self.lock_settings_write();
        let mut catalog = self.read_mcp_server_catalog()?;
        if catalog
            .servers
            .iter()
            .any(|current| current.id == server.id)
        {
            return Err(RuntimeError::Conflict(format!(
                "MCP server id already exists: {}",
                server.id
            )));
        }
        ensure_name_is_unique(&catalog.servers, &server, None)?;
        catalog.servers.push(server.clone());
        atomic::write_json(&self.mcp_servers_path(), &catalog)?;
        Ok(server)
    }

    pub fn update_mcp_server(
        &self,
        server: McpServerDefinition,
        expected_secret_names: &[String],
    ) -> Result<McpServerDefinition, RuntimeError> {
        let _guard = self.lock_settings_write();
        let mut catalog = self.read_mcp_server_catalog()?;
        let index = catalog
            .servers
            .iter()
            .position(|current| current.id == server.id)
            .ok_or_else(|| mcp_server_not_found(&server.id))?;
        ensure_secret_names_match(&catalog.servers[index], expected_secret_names)?;
        ensure_name_is_unique(&catalog.servers, &server, Some(index))?;
        catalog.servers[index] = server.clone();
        atomic::write_json(&self.mcp_servers_path(), &catalog)?;
        Ok(server)
    }

    pub fn set_mcp_server_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<McpServerDefinition, RuntimeError> {
        let _guard = self.lock_settings_write();
        let mut catalog = self.read_mcp_server_catalog()?;
        let updated = catalog
            .servers
            .iter_mut()
            .find(|server| server.id == id)
            .ok_or_else(|| mcp_server_not_found(id))?;
        updated.enabled = enabled;
        let updated = updated.clone();
        atomic::write_json(&self.mcp_servers_path(), &catalog)?;
        Ok(updated)
    }

    pub fn delete_mcp_server(
        &self,
        id: &str,
        expected_secret_names: &[String],
    ) -> Result<Vec<String>, RuntimeError> {
        let _guard = self.lock_settings_write();
        let mut catalog = self.read_mcp_server_catalog()?;
        let index = catalog
            .servers
            .iter()
            .position(|server| server.id == id)
            .ok_or_else(|| mcp_server_not_found(id))?;
        ensure_secret_names_match(&catalog.servers[index], expected_secret_names)?;
        let removed = catalog.servers.remove(index);
        atomic::write_json(&self.mcp_servers_path(), &catalog)?;
        Ok(secret_names(&removed))
    }

    fn read_mcp_server_catalog(&self) -> Result<StoredMcpServers, RuntimeError> {
        let path = self.mcp_servers_path();
        if !path.exists() {
            return Ok(StoredMcpServers {
                version: MCP_SERVERS_SCHEMA_VERSION,
                servers: Vec::new(),
            });
        }
        let catalog: StoredMcpServers = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if catalog.version != MCP_SERVERS_SCHEMA_VERSION {
            return Err(RuntimeError::Storage(format!(
                "unsupported MCP server settings version: {}",
                catalog.version
            )));
        }
        Ok(catalog)
    }

    fn mcp_servers_path(&self) -> std::path::PathBuf {
        self.settings_dir().join("mcp_servers.json")
    }
}

const fn mcp_servers_schema_version() -> u32 {
    MCP_SERVERS_SCHEMA_VERSION
}

fn ensure_secret_names_match(
    current: &McpServerDefinition,
    expected: &[String],
) -> Result<(), RuntimeError> {
    let mut expected = expected.to_vec();
    expected.sort();
    expected.dedup();
    if secret_names(current) == expected {
        return Ok(());
    }
    Err(RuntimeError::Conflict(
        "MCP server secret fields changed since the editor was opened".to_string(),
    ))
}

fn ensure_name_is_unique(
    servers: &[McpServerDefinition],
    candidate: &McpServerDefinition,
    candidate_index: Option<usize>,
) -> Result<(), RuntimeError> {
    let name = server_name_key(&candidate.label);
    let duplicate = servers.iter().enumerate().any(|(index, current)| {
        Some(index) != candidate_index
            && current.scope == candidate.scope
            && server_name_key(&current.label) == name
    });
    if duplicate {
        return Err(RuntimeError::Conflict(format!(
            "MCP server name already exists in this scope: {}",
            candidate.label.trim()
        )));
    }
    Ok(())
}

fn server_name_key(label: &str) -> String {
    label.trim().to_lowercase()
}

fn secret_names(server: &McpServerDefinition) -> Vec<String> {
    let mut names = match &server.configuration {
        McpServerConfiguration::Stdio { secret_env, .. } => secret_env.clone(),
        McpServerConfiguration::Http { secret_headers, .. }
        | McpServerConfiguration::Sse { secret_headers, .. } => secret_headers.clone(),
    };
    names.sort();
    names.dedup();
    names
}

fn mcp_server_not_found(id: &str) -> RuntimeError {
    RuntimeError::InvalidParams(format!("MCP server not found: {id}"))
}

#[cfg(test)]
#[path = "mcp_servers_tests.rs"]
mod tests;
