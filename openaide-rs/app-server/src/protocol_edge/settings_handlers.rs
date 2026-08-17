use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::events::{AppServerEventPayload, TaskChanges};
use openaide_app_server_protocol::settings::{
    AppPreferencesParams, AppPreferencesResult, AppPreferencesUpdateParams, McpCreateServerParams,
    McpDeleteServerParams, McpGetServerDetailsParams, McpGetServerDetailsResult, McpMutationResult,
    McpSetServerEnabledParams, McpUpdateServerParams, NewTaskDefaultsUpdateParams,
    ResetTaskHistoryParams, ResetTaskHistoryResult, RuntimeSettingsParams, RuntimeSettingsResult,
    RuntimeSettingsUpdateParams, SettingsMcpServersParams, SettingsMcpServersResult,
    SettingsSkillDetailsParams, SettingsSkillDetailsResult, SettingsSkillsParams,
    SettingsSkillsResult,
};
use openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot;
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_settings_reset_task_history(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: crate::client_lifecycle::AppServerTime,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<ResetTaskHistoryParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        let removed_tasks = match self.task_history.reset_task_history() {
            Ok(tasks) => tasks,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let mut events = removed_tasks
            .into_iter()
            .flat_map(|task| {
                self.publish_task_payload(
                    &task.task_id,
                    AppServerEventPayload::TaskChanged {
                        task_id: task.task_id.clone(),
                        revision: task.next_revision,
                        changes: TaskChanges {
                            removed: true,
                            ..TaskChanges::default()
                        },
                    },
                    now,
                )
            })
            .collect::<Vec<_>>();
        events.extend(self.publish_navigation_replacement(now));
        responses::result_with_events(
            connection_id,
            id,
            meta,
            ResetTaskHistoryResult::default(),
            events,
        )
    }

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

    pub(super) fn handle_settings_update_new_task_defaults(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<NewTaskDefaultsUpdateParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.new_task_defaults.update_defaults(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<NewTaskDefaultsSnapshot>(connection_id, id, meta, result)
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
