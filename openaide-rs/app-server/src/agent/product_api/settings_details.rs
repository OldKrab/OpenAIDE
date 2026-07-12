use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use openaide_app_server_protocol::agent::{
    AgentSettingsDetail, AgentSettingsDetailsParams, AgentSettingsDetailsResult,
    AgentSettingsEnvRow, AgentSettingsSourceKind, AgentSettingsStatus, AgentSettingsTransport,
};
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::AgentId;
use openaide_app_server_protocol::snapshot::AgentStatus;

use crate::agent::registry::AgentCatalogRecord;
use crate::agent::registry_builtin::{BuiltInAgentMetadata, BUILT_IN_AGENT_METADATA};

use super::{protocol_error_from_runtime, AgentProductApi};

pub(crate) trait AgentSettingsDetailsWorkflow: Send + Sync {
    fn agent_settings_details(
        &self,
        params: AgentSettingsDetailsParams,
    ) -> Result<AgentSettingsDetailsResult, ProtocolError>;
}

impl AgentSettingsDetailsWorkflow for AgentProductApi {
    fn agent_settings_details(
        &self,
        _params: AgentSettingsDetailsParams,
    ) -> Result<AgentSettingsDetailsResult, ProtocolError> {
        let records = self
            .catalog_store
            .load_records()
            .map_err(protocol_error_from_runtime)?;
        refresh_enabled_agent_statuses(&records, self);
        Ok(AgentSettingsDetailsResult {
            generated_at: generated_at(),
            agents: details_from_catalog(&records, self),
        })
    }
}

fn refresh_enabled_agent_statuses(records: &[AgentCatalogRecord], api: &AgentProductApi) {
    for agent_id in enabled_agent_ids(records) {
        let probe = api.gateway.probe(crate::agent::AgentProbeRequest {
            agent_id: agent_id.clone(),
        });
        let _ = api.record_probe_result(&agent_id, probe);
    }
}

fn enabled_agent_ids(records: &[AgentCatalogRecord]) -> Vec<String> {
    let mut overlays = HashMap::new();
    for record in records {
        if let Ok(id) = record.id() {
            overlays.insert(id, record);
        }
    }

    let mut ids: Vec<_> = BUILT_IN_AGENT_METADATA
        .iter()
        .filter(|metadata| {
            overlays
                .get(metadata.id)
                .map(|record| record.enabled())
                .unwrap_or(true)
        })
        .map(|metadata| metadata.id.to_string())
        .collect();

    ids.extend(records.iter().filter_map(|record| {
        (record.is_custom() && record.enabled())
            .then(|| record.id().ok())
            .flatten()
    }));
    ids.sort();
    ids.dedup();
    ids
}

fn details_from_catalog(
    records: &[AgentCatalogRecord],
    api: &AgentProductApi,
) -> Vec<AgentSettingsDetail> {
    let mut overlays = HashMap::new();
    for record in records {
        if let Ok(id) = record.id() {
            overlays.insert(id, record);
        }
    }

    let mut details: Vec<_> = BUILT_IN_AGENT_METADATA
        .iter()
        .map(|metadata| built_in_detail(metadata, overlays.get(metadata.id).copied(), api))
        .collect();

    for record in records {
        if record.is_custom() {
            if let Some(detail) = custom_detail(record, api) {
                details.push(detail);
            }
        }
    }
    details
}

fn built_in_detail(
    metadata: &BuiltInAgentMetadata,
    overlay: Option<&AgentCatalogRecord>,
    api: &AgentProductApi,
) -> AgentSettingsDetail {
    let enabled = overlay.map(AgentCatalogRecord::enabled).unwrap_or(true);
    AgentSettingsDetail {
        agent_id: AgentId::from(metadata.id.to_string()),
        label: metadata.label.to_string(),
        enabled,
        source_kind: AgentSettingsSourceKind::BuiltIn,
        icon: metadata.icon.to_string(),
        transport: AgentSettingsTransport::Stdio,
        status: status_for(metadata.id, enabled, api),
        launch_label: "Built-in stdio launch policy".to_string(),
        command_line: None,
        env: Vec::new(),
        description: metadata.description.to_string(),
        capabilities: base_capabilities(),
        auth_methods: Vec::new(),
    }
}

fn custom_detail(
    record: &AgentCatalogRecord,
    api: &AgentProductApi,
) -> Option<AgentSettingsDetail> {
    let id = record.id().ok()?;
    let command_line = if record.command_line().trim().is_empty() {
        command_line(record.command(), record.args())
    } else {
        record.command_line().to_string()
    };
    Some(AgentSettingsDetail {
        agent_id: AgentId::from(id.clone()),
        label: trimmed_or_id(record.label(), &id)
            .chars()
            .take(80)
            .collect(),
        enabled: record.enabled(),
        source_kind: AgentSettingsSourceKind::Custom,
        icon: custom_icon(record.icon()),
        transport: AgentSettingsTransport::Stdio,
        status: status_for(&id, record.enabled(), api),
        launch_label: record.command().to_string(),
        command_line: Some(command_line),
        env: env_rows(record),
        description: "Custom ACP stdio Agent".to_string(),
        capabilities: base_capabilities(),
        auth_methods: Vec::new(),
    })
}

fn status_for(agent_id: &str, enabled: bool, api: &AgentProductApi) -> AgentSettingsStatus {
    if !enabled {
        return AgentSettingsStatus::Disabled;
    }
    match api.statuses.snapshot(agent_id).status {
        AgentStatus::Disconnected => AgentSettingsStatus::Disconnected,
        AgentStatus::Launching => AgentSettingsStatus::Launching,
        AgentStatus::Connected => AgentSettingsStatus::Connected,
        AgentStatus::SetupRequired => AgentSettingsStatus::SetupRequired,
        AgentStatus::AuthRequired => AgentSettingsStatus::AuthRequired,
        AgentStatus::Unsupported => AgentSettingsStatus::Unsupported,
        AgentStatus::Failed => AgentSettingsStatus::Failed,
    }
}

fn env_rows(record: &AgentCatalogRecord) -> Vec<AgentSettingsEnvRow> {
    let mut rows: Vec<_> = record
        .env()
        .iter()
        .map(|(name, value)| AgentSettingsEnvRow {
            name: name.clone(),
            value: Some(value.clone()),
            secret: false,
        })
        .collect();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows.extend(record.secret_env().iter().map(|name| AgentSettingsEnvRow {
        name: name.clone(),
        value: None,
        secret: true,
    }));
    rows
}

fn command_line(command: &str, args: &[String]) -> String {
    std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .map(shell_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_token(value: &str) -> String {
    if value
        .chars()
        .all(|item| item.is_ascii_alphanumeric() || matches!(item, '_' | '-' | '.' | '/' | ':'))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn custom_icon(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "bot".to_string()
    } else {
        value.chars().take(40).collect()
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

fn base_capabilities() -> Vec<String> {
    vec![
        "ACP stdio".to_string(),
        "Configuration Options".to_string(),
        "Filesystem bridge".to_string(),
        "Terminal bridge".to_string(),
    ]
}

fn generated_at() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("unix:{millis}")
}

#[cfg(test)]
#[path = "settings_details_tests.rs"]
mod tests;
