use openaide_app_server_protocol::devices::{
    DevicesApproveJoinRequestParams, DevicesApproveJoinRequestResult, DevicesCancelInviteParams,
    DevicesCancelInviteResult, DevicesCreateInviteParams, DevicesPreviewJoinRequestParams,
    DevicesRemoveParams, DevicesRemoveResult,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{event_deliveries, responses, GatewayEventDelivery, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_devices_create_invite(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<DevicesCreateInviteParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        let added_by = self.pairing_client_label(&connection_id);
        match self.remote_devices.create_invite(added_by, now.0) {
            Ok(result) => {
                crate::logging::info("remote_device_invite_created", serde_json::json!({}));
                let events = self.publish_device_collection_update(now);
                self.result_with_events(connection_id, id, meta, result, events)
            }
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    pub(super) fn handle_devices_cancel_invite(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<DevicesCancelInviteParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        self.remote_devices.cancel_invite();
        let events = self.publish_device_collection_update(now);
        self.result_with_events(
            connection_id,
            id,
            meta,
            DevicesCancelInviteResult {},
            events,
        )
    }

    pub(super) fn handle_devices_preview_join_request(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<DevicesPreviewJoinRequestParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        match self.remote_devices.preview_join_request(&params.code) {
            Ok(result) => self.result(connection_id, id, meta, result),
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    pub(super) fn handle_devices_approve_join_request(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<DevicesApproveJoinRequestParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let added_by = self.pairing_client_label(&connection_id);
        match self
            .remote_devices
            .approve_join_request(&params.code, added_by, now.0)
        {
            Ok(()) => {
                crate::logging::info("remote_device_join_request_approved", serde_json::json!({}));
                let events = self.publish_device_collection_update(now);
                self.result_with_events(
                    connection_id,
                    id,
                    meta,
                    DevicesApproveJoinRequestResult {},
                    events,
                )
            }
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    pub(super) fn handle_devices_remove(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<DevicesRemoveParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        match self.remote_devices.remove(&params.device_id) {
            Ok(()) => {
                // Same short key prefix the edge logs, so a removal lines up with its disconnect.
                let device: String = params.device_id.chars().take(10).collect();
                crate::logging::info(
                    "remote_device_removed",
                    serde_json::json!({ "device": device }),
                );
                let events = self.publish_device_collection_update(now);
                self.result_with_events(connection_id, id, meta, DevicesRemoveResult {}, events)
            }
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    /// Republishes after a change the network edge made, such as a device connecting.
    pub(crate) fn publish_background_device_collection_update(
        &mut self,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let events = self.publish_device_collection_update(now);
        // Links drain background publications on their next wake.
        self.pending_event_deliveries.extend(events.clone());
        events
    }

    fn publish_device_collection_update(
        &mut self,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let devices = self.remote_devices.snapshot();
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::DeviceCollectionUpdated { devices },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }

    /// Names the client that pairs a device: a Remote Device by its own name, a
    /// same-machine App Shell by the computer's.
    fn pairing_client_label(&self, connection_id: &ConnectionId) -> String {
        self.remote_devices
            .device_name_for_connection(connection_id.as_str())
            .unwrap_or_else(|| self.remote_devices.snapshot().server_name)
    }
}
