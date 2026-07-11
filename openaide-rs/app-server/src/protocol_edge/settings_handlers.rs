use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::settings::{
    AppPreferencesParams, AppPreferencesResult, AppPreferencesUpdateParams, RuntimeSettingsParams,
    RuntimeSettingsResult, RuntimeSettingsUpdateParams, SettingsMcpServersParams,
    SettingsMcpServersResult, SettingsSkillsParams, SettingsSkillsResult,
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
