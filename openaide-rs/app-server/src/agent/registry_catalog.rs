use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::registry::{AgentDefinition, AgentLaunch, AgentSourceKind};
use crate::agent::registry_builtin;
use crate::protocol::errors::RuntimeError;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct AgentCatalogRecord {
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default = "default_custom_icon")]
    icon: String,
    #[serde(default)]
    source_kind: AgentCatalogSourceKind,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    #[serde(default = "stdio_transport")]
    transport: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    command_line: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    secret_env: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum AgentCatalogSourceKind {
    #[default]
    BuiltIn,
    Custom,
}

impl AgentCatalogRecord {
    // Catalog construction keeps every persisted launch field explicit so new
    // identity-affecting fields cannot be silently defaulted at this boundary.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn custom(
        id: String,
        label: String,
        icon: String,
        enabled: bool,
        command: String,
        command_line: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        secret_env: Vec<String>,
    ) -> Self {
        Self {
            id,
            label,
            icon,
            source_kind: AgentCatalogSourceKind::Custom,
            enabled,
            transport: stdio_transport(),
            command,
            command_line,
            args,
            env,
            secret_env,
        }
    }

    pub(crate) fn disabled_builtin(id: String) -> Self {
        Self {
            id,
            label: String::new(),
            icon: String::new(),
            source_kind: AgentCatalogSourceKind::BuiltIn,
            enabled: false,
            transport: stdio_transport(),
            command: String::new(),
            command_line: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            secret_env: Vec::new(),
        }
    }

    pub(crate) fn id(&self) -> Result<String, RuntimeError> {
        record_id(self)
    }

    pub(crate) fn is_custom(&self) -> bool {
        matches!(self.source_kind, AgentCatalogSourceKind::Custom)
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn icon(&self) -> &str {
        &self.icon
    }

    pub(crate) fn command(&self) -> &str {
        &self.command
    }

    pub(crate) fn command_line(&self) -> &str {
        &self.command_line
    }

    pub(crate) fn args(&self) -> &[String] {
        &self.args
    }

    pub(crate) fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    pub(crate) fn secret_env(&self) -> &[String] {
        &self.secret_env
    }

    pub(crate) fn normalized_label_key(&self) -> String {
        normalized_label_key(trimmed_or_id(&self.label, &self.id))
    }

    pub(crate) fn normalized_launch_command_key(&self) -> String {
        normalized_launch_command_key(
            std::iter::once(self.command.as_str()).chain(self.args.iter().map(String::as_str)),
        )
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn same_launch_identity(&self, other: &Self) -> bool {
        self.transport == other.transport
            && self.command == other.command
            && self.command_line == other.command_line
            && self.args == other.args
            && self.env == other.env
            && self.secret_env == other.secret_env
    }

    pub(crate) fn set_metadata(&mut self, label: String, icon: String, enabled: bool) {
        self.label = label;
        self.icon = icon;
        self.enabled = enabled;
    }
}

pub(super) fn definition_from_record(
    record: AgentCatalogRecord,
) -> Result<Option<AgentDefinition>, RuntimeError> {
    if !record.enabled {
        return Ok(None);
    }

    let AgentCatalogRecord {
        id: raw_id,
        label,
        icon: _,
        source_kind,
        enabled: _,
        transport,
        command,
        command_line: _,
        args,
        env,
        secret_env,
    } = record;

    let id = normalized_agent_id(&raw_id)
        .ok_or_else(|| RuntimeError::InvalidParams("agents.id".to_string()))?;
    if transport != "stdio" || command.trim().is_empty() {
        return Err(RuntimeError::InvalidParams(format!("agents.{id}.command")));
    }

    let source_kind = match source_kind {
        AgentCatalogSourceKind::BuiltIn => AgentSourceKind::BuiltIn,
        AgentCatalogSourceKind::Custom => AgentSourceKind::Custom,
    };
    let launch = match source_kind {
        AgentSourceKind::BuiltIn => registry_builtin::known_built_in_launch(&id)
            .unwrap_or_else(|| custom_launch(&id, command, args, env, secret_env)),
        AgentSourceKind::Custom => custom_launch(&id, command, args, env, secret_env),
    };

    Ok(Some(AgentDefinition::new(
        id,
        trimmed_or_id(&label, &raw_id).chars().take(80).collect(),
        source_kind,
        AgentLaunch::AcpStdio(launch),
    )))
}

pub(super) fn record_enabled(record: &AgentCatalogRecord) -> bool {
    record.enabled
}

pub(super) fn record_id(record: &AgentCatalogRecord) -> Result<String, RuntimeError> {
    normalized_agent_id(&record.id).ok_or_else(|| RuntimeError::InvalidParams("agents.id".into()))
}

fn custom_launch(
    id: &str,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    secret_env: Vec<String>,
) -> AcpAgentConfig {
    AcpAgentConfig {
        agent_id: id.to_string(),
        command,
        args,
        env: env.into_iter().collect(),
        secret_env,
    }
}

fn enabled_by_default() -> bool {
    true
}

fn stdio_transport() -> String {
    "stdio".to_string()
}

fn default_custom_icon() -> String {
    "bot".to_string()
}

fn normalized_agent_id(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.is_empty()
        && value
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || matches!(item, '_' | '-' | '.'))
    {
        Some(value.to_string())
    } else {
        None
    }
}

fn trimmed_or_id<'a>(label: &'a str, id: &'a str) -> &'a str {
    let label = label.trim();
    if label.is_empty() {
        id
    } else {
        label
    }
}

pub(super) fn normalized_label_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub(super) fn normalized_launch_command_key<'a>(
    parts: impl IntoIterator<Item = &'a str>,
) -> String {
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\0")
}
