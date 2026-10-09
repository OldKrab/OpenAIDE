use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::task::{
    TaskStopBackgroundCommandParams, TaskStopBackgroundCommandResult,
};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_task_stop_background_command(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskStopBackgroundCommandParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for background command stop");
        if let Err(error) = self
            .task_cancel
            .stop_background_command_for_client(&client.client_instance_id, params)
        {
            return self.error(connection_id, id, meta, error);
        }
        self.result::<TaskStopBackgroundCommandResult>(
            connection_id,
            id,
            meta,
            TaskStopBackgroundCommandResult {},
        )
    }
}
