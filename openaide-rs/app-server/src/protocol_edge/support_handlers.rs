use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::support::{
    SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult,
};
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_support_recover_stuck_sessions(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<SupportRecoverStuckSessionsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.task_cancel.recover_stuck_sessions(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let mut events = Vec::new();
        for task in &result.recovered_tasks {
            events.extend(self.publish_task_updates(task, now));
        }
        self.result_with_events::<SupportRecoverStuckSessionsResult>(
            connection_id,
            id,
            meta,
            result,
            events,
        )
    }
}
