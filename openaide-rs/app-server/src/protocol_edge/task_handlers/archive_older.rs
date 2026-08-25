use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::task::{TaskArchiveOlderParams, TaskArchiveOlderResult};
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::protocol_edge::{responses, GatewayOutcome};

use super::RpcGateway;

impl RpcGateway {
    pub(in crate::protocol_edge) fn handle_task_archive_older(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskArchiveOlderParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let preview = params.preview;
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for bulk task archive");
        let result = match self
            .task_archive
            .archive_older_for_client(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = if preview {
            Vec::new()
        } else {
            self.publish_project_entries_replaced(&result.project_id, now)
        };
        self.result_with_events::<TaskArchiveOlderResult>(connection_id, id, meta, result, events)
    }
}
