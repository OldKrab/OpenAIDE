use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::settings::{
    AppPreferencesParams, AppPreferencesResult, AppPreferencesUpdateParams, McpCreateServerParams,
    McpDeleteServerParams, McpGetServerDetailsParams, McpGetServerDetailsResult, McpMutationResult,
    McpSetServerEnabledParams, McpUpdateServerParams, RuntimeSettingsParams, RuntimeSettingsResult,
    RuntimeSettingsUpdateParams, SettingsMcpServersParams, SettingsMcpServersResult,
    SettingsSkillDetailsParams, SettingsSkillDetailsResult, SettingsSkillsParams,
    SettingsSkillsResult,
};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_settings_get_mcp_servers(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<SettingsMcpServersParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.mcp_servers_settings.mcp_servers_settings(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<SettingsMcpServersResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_mcp_get_server_details(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<McpGetServerDetailsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.mcp_servers_settings.mcp_server_details(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<McpGetServerDetailsResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_mcp_create_server(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<McpCreateServerParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.mcp_servers_settings.create_mcp_server(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<McpMutationResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_mcp_update_server(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<McpUpdateServerParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.mcp_servers_settings.update_mcp_server(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<McpMutationResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_mcp_delete_server(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<McpDeleteServerParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.mcp_servers_settings.delete_mcp_server(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<McpMutationResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_mcp_set_server_enabled(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<McpSetServerEnabledParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.mcp_servers_settings.set_mcp_server_enabled(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<McpMutationResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_settings_get_skills(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<SettingsSkillsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.skills_settings.skills_settings(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<SettingsSkillsResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_settings_get_skill_details(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<SettingsSkillDetailsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.skills_settings.skill_details(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<SettingsSkillDetailsResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_settings_get_preferences(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AppPreferencesParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.app_preferences.app_preferences(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AppPreferencesResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_settings_update_preferences(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AppPreferencesUpdateParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.app_preferences.update_app_preferences(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AppPreferencesResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_settings_get_runtime(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<RuntimeSettingsParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        let result = match self.runtime_settings.runtime_settings() {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<RuntimeSettingsResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_settings_update_runtime(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<RuntimeSettingsUpdateParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.runtime_settings.update_runtime_settings(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<RuntimeSettingsResult>(connection_id, id, meta, result)
    }
}
