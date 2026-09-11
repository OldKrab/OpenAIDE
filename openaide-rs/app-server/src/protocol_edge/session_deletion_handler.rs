use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::task::{NativeSessionDeleteParams, NativeSessionDeleteResult};
use serde_json::Value;

use super::{responses, GatewayOutcome, RpcGateway};
use crate::client_lifecycle::{AppServerTime, ConnectionId};

impl RpcGateway {
    pub(super) fn handle_native_session_delete(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<NativeSessionDeleteParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for Native Session deletion");
        let result = match self
            .task_archive
            .delete_native_session_for_client(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = match &result {
            NativeSessionDeleteResult::Deleted { project_id, .. } => {
                self.publish_project_entries_replaced(project_id, now)
            }
            NativeSessionDeleteResult::ConfirmationRequired { .. } => Vec::new(),
        };
        self.result_with_events(connection_id, id, meta, result, events)
    }
}
