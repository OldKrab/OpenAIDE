use openaide_app_server_protocol::client::{
    ClientCapabilitiesChangedParams, ClientCapabilitiesChangedResult, ClientHeartbeatParams,
    ClientHeartbeatResult,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    /// Authenticated transport activity proves the initialized client is still live.
    pub(crate) fn observe_connection_activity(
        &mut self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> bool {
        let Some(client_instance_id) = self
            .client_hub
            .observe_connection_activity(connection_id, now)
        else {
            return false;
        };
        self.attachments.keep_alive_for_client(&client_instance_id);
        true
    }

    pub(super) fn handle_client_capabilities_changed(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ClientCapabilitiesChangedParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(
                    openaide_app_server_protocol::methods::CLIENT_CAPABILITIES_CHANGED.to_string(),
                ),
            );
        };

        if let Some(capabilities) = params.capabilities {
            self.client_hub
                .update_capabilities(&context.client_instance_id, capabilities);
        }
        let projects_changed = params.workspace_roots.is_some_and(|roots| {
            self.project_roots.replace_client_workspace_roots(
                &context.client_instance_id,
                roots.into_iter().map(|root| root.path),
            )
        });
        let projects = match self.snapshots.project_collection_snapshot() {
            Ok(projects) => projects,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = if projects_changed {
            self.publish_project_collection_update(now)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        self.result_with_events(
            connection_id,
            id,
            meta,
            ClientCapabilitiesChangedResult { projects },
            events,
        )
    }

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
