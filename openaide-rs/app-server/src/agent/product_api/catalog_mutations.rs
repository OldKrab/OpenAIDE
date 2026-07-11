use std::collections::HashMap;

use openaide_app_server_protocol::agent::{
    AgentCreateCustomParams, AgentCreateCustomResult, AgentDeleteCustomParams,
    AgentDeleteCustomResult, AgentReplaceCustomCleanup, AgentReplaceCustomHistoryPolicy,
    AgentReplaceCustomParams, AgentReplaceCustomResult, AgentSetEnabledParams,
    AgentSetEnabledResult, AgentUpdateCustomMetadataParams, AgentUpdateCustomMetadataResult,
};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};

use crate::agent::registry::AgentCatalogRecord;
use crate::agent_identity::{
    generated_custom_agent_id, normalized_existing_custom_agent_id, normalized_icon,
    normalized_label, valid_env_name, AgentIdentityValidationError,
};

use super::{protocol_error_from_runtime, AgentProductApi};

pub(crate) trait AgentCatalogMutationWorkflow: Send + Sync {
    fn create_custom(
        &self,
        params: AgentCreateCustomParams,
    ) -> Result<AgentCreateCustomResult, ProtocolError>;

    fn update_custom_metadata(
        &self,
        params: AgentUpdateCustomMetadataParams,
    ) -> Result<AgentUpdateCustomMetadataResult, ProtocolError>;

    fn replace_custom(
        &self,
        params: AgentReplaceCustomParams,
    ) -> Result<AgentReplaceCustomResult, ProtocolError>;

    fn delete_custom(
        &self,
        params: AgentDeleteCustomParams,
    ) -> Result<AgentDeleteCustomResult, ProtocolError>;

    fn set_enabled(
        &self,
        params: AgentSetEnabledParams,
    ) -> Result<AgentSetEnabledResult, ProtocolError>;
}

impl AgentCatalogMutationWorkflow for AgentProductApi {
    fn create_custom(
        &self,
        params: AgentCreateCustomParams,
    ) -> Result<AgentCreateCustomResult, ProtocolError> {
        let agent_id = requested_or_generated_custom_agent_id(params.agent_id.clone(), "agentId")?;
        ensure_custom_id_available(self, &agent_id, "agentId")?;
        let record = custom_catalog_record(agent_id.as_str(), CustomLaunchParams::Create(params))?;
        let registry = self
            .catalog_store
            .save_custom(record)
            .map_err(protocol_error_from_runtime)?;
        self.registry.replace(registry);
        self.statuses.clear(agent_id.as_str());
        Ok(AgentCreateCustomResult {
            agent_id,
            agents: self.snapshot()?,
        })
    }

    fn update_custom_metadata(
        &self,
        params: AgentUpdateCustomMetadataParams,
    ) -> Result<AgentUpdateCustomMetadataResult, ProtocolError> {
        let agent_id = normalized_existing_custom_agent_id(params.agent_id)
            .map_err(protocol_error_from_identity)?;
        let label = normalized_label(params.label).map_err(protocol_error_from_identity)?;
        let registry = self
            .catalog_store
            .update_custom_metadata(
                agent_id.as_str(),
                label,
                normalized_icon(params.icon),
                params.enabled,
            )
            .map_err(protocol_error_from_runtime)?;
        self.registry.replace(registry);
        self.statuses.clear(agent_id.as_str());
        Ok(AgentUpdateCustomMetadataResult {
            agent_id,
            agents: self.snapshot()?,
        })
    }

    fn replace_custom(
        &self,
        params: AgentReplaceCustomParams,
    ) -> Result<AgentReplaceCustomResult, ProtocolError> {
        if !params.confirmation.accepted_launch_identity_change {
            return Err(validation_error(
                "confirmation.acceptedLaunchIdentityChange",
            ));
        }
        let old_agent_id = normalized_existing_custom_agent_id(params.source_agent_id.clone())
            .map_err(protocol_error_from_identity)?;
        let removed_secret_env = custom_secret_env(self, old_agent_id.as_str())?;
        validate_expected_secret_env(
            params.expected_source_secret_env.as_deref(),
            &removed_secret_env,
            "expectedSourceSecretEnv",
        )?;
        let new_agent_id = requested_or_generated_custom_agent_id(
            params.target_agent_id.clone(),
            "targetAgentId",
        )?;
        ensure_custom_id_available(self, &new_agent_id, "targetAgentId")?;
        let record =
            custom_catalog_record(new_agent_id.as_str(), CustomLaunchParams::Replace(params))?;
        let registry = self
            .catalog_store
            .replace_custom(old_agent_id.as_str(), record)
            .map_err(protocol_error_from_runtime)?;
        self.registry.replace(registry);
        let removed_cached_status = self.statuses.clear(old_agent_id.as_str());
        self.statuses.clear(new_agent_id.as_str());
        Ok(AgentReplaceCustomResult {
            old_agent_id,
            new_agent_id,
            cleanup: AgentReplaceCustomCleanup {
                removed_catalog_record: true,
                removed_cached_status,
                removed_settings_overlay: false,
                removed_secret_env,
                history_policy: AgentReplaceCustomHistoryPolicy::PreserveHistoricalTasks,
            },
            agents: self.snapshot()?,
        })
    }

    fn delete_custom(
        &self,
        params: AgentDeleteCustomParams,
    ) -> Result<AgentDeleteCustomResult, ProtocolError> {
        let agent_id = params.agent_id;
        let removed_secret_env = custom_secret_env(self, agent_id.as_str())?;
        validate_expected_secret_env(
            params.expected_secret_env.as_deref(),
            &removed_secret_env,
            "expectedSecretEnv",
        )?;
        let registry = self
            .catalog_store
            .delete_custom(agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        self.registry.replace(registry);
        self.statuses.clear(agent_id.as_str());
        Ok(AgentDeleteCustomResult {
            agent_id,
            removed_secret_env,
            agents: self.snapshot()?,
        })
    }

    fn set_enabled(
        &self,
        params: AgentSetEnabledParams,
    ) -> Result<AgentSetEnabledResult, ProtocolError> {
        let registry = self
            .catalog_store
            .set_enabled(params.agent_id.as_str(), params.enabled)
            .map_err(protocol_error_from_runtime)?;
        self.registry.replace(registry);
        self.statuses.clear(params.agent_id.as_str());
        Ok(AgentSetEnabledResult {
            agents: self.snapshot()?,
        })
    }
}

fn requested_or_generated_custom_agent_id(
    requested: Option<openaide_app_server_protocol::ids::AgentId>,
    field: &'static str,
) -> Result<openaide_app_server_protocol::ids::AgentId, ProtocolError> {
    match requested {
        Some(agent_id) => {
            normalized_existing_custom_agent_id(agent_id).map_err(|_| validation_error(field))
        }
        None => Ok(generated_custom_agent_id()),
    }
}

fn ensure_custom_id_available(
    api: &AgentProductApi,
    agent_id: &openaide_app_server_protocol::ids::AgentId,
    field: &'static str,
) -> Result<(), ProtocolError> {
    let records = api
        .catalog_store
        .load_records()
        .map_err(protocol_error_from_runtime)?;
    for record in records {
        if record.id().map_err(protocol_error_from_runtime)? == agent_id.as_str() {
            return Err(validation_error(field));
        }
    }
    Ok(())
}

fn validate_expected_secret_env(
    expected: Option<&[String]>,
    actual: &[String],
    field: &'static str,
) -> Result<(), ProtocolError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let mut expected = expected.to_vec();
    let mut actual = actual.to_vec();
    expected.sort_unstable();
    expected.dedup();
    actual.sort_unstable();
    actual.dedup();
    if expected == actual {
        Ok(())
    } else {
        Err(validation_error(field))
    }
}

fn custom_secret_env(api: &AgentProductApi, agent_id: &str) -> Result<Vec<String>, ProtocolError> {
    let records = api
        .catalog_store
        .load_records()
        .map_err(protocol_error_from_runtime)?;
    for record in records {
        if record.id().map_err(protocol_error_from_runtime)? == agent_id && record.is_custom() {
            return Ok(record.secret_env().to_vec());
        }
    }
    Ok(Vec::new())
}

fn custom_catalog_record(
    agent_id: &str,
    params: CustomLaunchParams,
) -> Result<AgentCatalogRecord, ProtocolError> {
    let label = normalized_label(params.label()).map_err(protocol_error_from_identity)?;
    let command = params.command().trim();
    if command.is_empty() {
        return Err(validation_error("command"));
    }
    let env = params
        .env()
        .clone()
        .into_iter()
        .map(|(name, value)| {
            if valid_env_name(&name) {
                Ok((name, value))
            } else {
                Err(validation_error("env"))
            }
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    if !params.secret_env().iter().all(|name| valid_env_name(name)) {
        return Err(validation_error("secretEnv"));
    }
    Ok(AgentCatalogRecord::custom(
        agent_id.to_string(),
        label,
        normalized_icon(params.icon()),
        params.enabled(),
        command.to_string(),
        params.command_line().trim().to_string(),
        params.args().to_vec(),
        env,
        params.secret_env().to_vec(),
    ))
}

enum CustomLaunchParams {
    Create(AgentCreateCustomParams),
    Replace(AgentReplaceCustomParams),
}

impl CustomLaunchParams {
    fn label(&self) -> String {
        match self {
            Self::Create(params) => params.label.clone(),
            Self::Replace(params) => params.label.clone(),
        }
    }

    fn icon(&self) -> String {
        match self {
            Self::Create(params) => params.icon.clone(),
            Self::Replace(params) => params.icon.clone(),
        }
    }

    fn command_line(&self) -> &str {
        match self {
            Self::Create(params) => &params.command_line,
            Self::Replace(params) => &params.command_line,
        }
    }

    fn command(&self) -> &str {
        match self {
            Self::Create(params) => &params.command,
            Self::Replace(params) => &params.command,
        }
    }

    fn args(&self) -> &[String] {
        match self {
            Self::Create(params) => &params.args,
            Self::Replace(params) => &params.args,
        }
    }

    fn env(&self) -> &std::collections::BTreeMap<String, String> {
        match self {
            Self::Create(params) => &params.env,
            Self::Replace(params) => &params.env,
        }
    }

    fn secret_env(&self) -> &[String] {
        match self {
            Self::Create(params) => &params.secret_env,
            Self::Replace(params) => &params.secret_env,
        }
    }

    fn enabled(&self) -> bool {
        match self {
            Self::Create(params) => params.enabled,
            Self::Replace(params) => params.enabled,
        }
    }
}

fn protocol_error_from_identity(error: AgentIdentityValidationError) -> ProtocolError {
    validation_error(error.field())
}

fn validation_error(field: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::ValidationFailed,
        message: format!("Invalid field: {field}"),
        recoverable: false,
        target: None,
    }
}
