use openaide_app_server_protocol::client::{ClientHeartbeatParams, ClientHeartbeatResult};
use openaide_app_server_protocol::envelopes::RequestMeta;
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_client_heartbeat(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<ClientHeartbeatParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        let events = self.drain_event_deliveries_for_connection(&connection_id);
        responses::result_with_events(connection_id, id, meta, ClientHeartbeatResult {}, events)
    }
}
