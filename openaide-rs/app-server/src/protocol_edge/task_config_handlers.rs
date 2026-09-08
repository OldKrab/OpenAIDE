use super::{responses, GatewayOutcome, RpcGateway};
use crate::client_lifecycle::{AppServerTime, ConnectionId};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::task::{
    TaskResolveConfigPreferencesParams, TaskResolveConfigPreferencesResult,
    TaskSetConfigOptionParams, TaskSetConfigOptionResult,
};
use serde_json::Value;

impl RpcGateway {
    pub(super) fn handle_task_set_config_option(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskSetConfigOptionParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for config changes");
        let agent_config = match self
            .task_set_config_option
            .set_config_option_for_client(&client.client_instance_id, params)
        {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskSetConfigOptionResult>(
            connection_id,
            id,
            meta,
            TaskSetConfigOptionResult { agent_config },
        )
    }

    pub(super) fn handle_task_resolve_config_preferences(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskResolveConfigPreferencesParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for config changes");
        let task = match self
            .task_set_config_option
            .resolve_config_preferences_for_client(&client.client_instance_id, params)
        {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskResolveConfigPreferencesResult>(
            connection_id,
            id,
            meta,
            TaskResolveConfigPreferencesResult { task },
        )
    }
}
