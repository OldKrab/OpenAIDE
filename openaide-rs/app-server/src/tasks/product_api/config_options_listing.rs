use openaide_app_server_protocol::agent::{
    AgentConfigOption, AgentConfigOptionCategory, AgentConfigOptionValue,
    AgentConfigOptionsCatalog, AgentConfigOptionsParams, AgentConfigOptionsResult,
    AgentConfigOptionsStatus, AgentSetConfigOptionParams,
};
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::AgentConfigOptionId;

use crate::agent::{AgentConfigOptionsRequest, AgentSetConfigOptionRequest};
use crate::projects::{resolve_project_context, ProjectTaskContext};
use crate::protocol::model::{
    ConfigOptionCategory as LegacyConfigOptionCategory,
    ConfigOptionsCatalog as LegacyConfigOptionsCatalog,
    ConfigOptionsStatus as LegacyConfigOptionsStatus,
};

use super::{protocol_error_from_runtime, AgentConfigOptionsWorkflow, TaskProductApi};

impl TaskProductApi {
    fn read_agent_config_options(
        &self,
        params: AgentConfigOptionsParams,
    ) -> Result<AgentConfigOptionsResult, ProtocolError> {
        let project = resolve_project_context(
            self.project_resolver.as_ref(),
            &params.project_id,
            params.workspace_root.as_deref(),
        )?;
        self.agent_registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        let catalog = self
            .agent_gateway
            .config_options(AgentConfigOptionsRequest {
                agent_id: params.agent_id.as_str().to_string(),
                cwd: project.workspace_root.clone(),
            })
            .map_err(protocol_error_from_runtime)?;
        Ok(config_result(params.agent_id, project, catalog))
    }

    fn update_agent_config_option(
        &self,
        params: AgentSetConfigOptionParams,
    ) -> Result<AgentConfigOptionsResult, ProtocolError> {
        let project = resolve_project_context(
            self.project_resolver.as_ref(),
            &params.project_id,
            params.workspace_root.as_deref(),
        )?;
        self.agent_registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        let catalog = self
            .agent_gateway
            .set_config_option(AgentSetConfigOptionRequest {
                agent_id: params.agent_id.as_str().to_string(),
                cwd: project.workspace_root.clone(),
                config_id: params.config_id.into_string(),
                value: params.value,
            })
            .map_err(protocol_error_from_runtime)?;
        Ok(config_result(params.agent_id, project, catalog))
    }
}

impl AgentConfigOptionsWorkflow for TaskProductApi {
    fn config_options(
        &self,
        params: AgentConfigOptionsParams,
    ) -> Result<AgentConfigOptionsResult, ProtocolError> {
        self.read_agent_config_options(params)
    }

    fn set_config_option(
        &self,
        params: AgentSetConfigOptionParams,
    ) -> Result<AgentConfigOptionsResult, ProtocolError> {
        self.update_agent_config_option(params)
    }
}

fn config_result(
    agent_id: openaide_app_server_protocol::ids::AgentId,
    project: ProjectTaskContext,
    catalog: LegacyConfigOptionsCatalog,
) -> AgentConfigOptionsResult {
    AgentConfigOptionsResult {
        agent_id,
        project_id: project.project_id,
        project_label: project.label,
        catalog: map_catalog(catalog),
    }
}

fn map_catalog(catalog: LegacyConfigOptionsCatalog) -> AgentConfigOptionsCatalog {
    AgentConfigOptionsCatalog {
        agent_id: catalog.agent_id.into(),
        status: match catalog.status {
            LegacyConfigOptionsStatus::Ready => AgentConfigOptionsStatus::Ready,
            LegacyConfigOptionsStatus::Empty => AgentConfigOptionsStatus::Empty,
        },
        options: catalog
            .options
            .into_iter()
            .map(|option| AgentConfigOption {
                id: AgentConfigOptionId::from(option.id),
                label: option.label,
                description: option.description,
                category: option.category.map(map_category),
                current_value: option.current_value,
                values: option
                    .values
                    .into_iter()
                    .map(|value| AgentConfigOptionValue {
                        id: value.id,
                        label: value.label,
                        description: value.description,
                        group_id: value.group_id,
                        group_label: value.group_label,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn map_category(category: LegacyConfigOptionCategory) -> AgentConfigOptionCategory {
    match category {
        LegacyConfigOptionCategory::Mode => AgentConfigOptionCategory::Mode,
        LegacyConfigOptionCategory::Model => AgentConfigOptionCategory::Model,
        LegacyConfigOptionCategory::ThoughtLevel => AgentConfigOptionCategory::ThoughtLevel,
        LegacyConfigOptionCategory::Other => AgentConfigOptionCategory::Other,
    }
}
