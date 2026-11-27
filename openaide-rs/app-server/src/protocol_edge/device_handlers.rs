use openaide_app_server_protocol::devices::{
    DevicesCancelInviteParams, DevicesCancelInviteResult, DevicesCreateInviteParams,
    DevicesRelayAddPersonParams, DevicesRelayPeopleParams, DevicesRelayRevokePersonParams,
    DevicesRelayRevokePersonResult, DevicesRemoveParams, DevicesRemoveResult,
    DevicesSetRelayParams,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::methods::{
    DEVICES_RELAY_ADD_PERSON, DEVICES_RELAY_PEOPLE, DEVICES_RELAY_REVOKE_PERSON, DEVICES_SET_RELAY,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::remote_devices::RemoteDevicesWorkflow;

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::global_state::GlobalAreas;

use super::{responses, GatewayOutcome, RpcGateway};

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
                self.publish_global_state(GlobalAreas::DEVICES);
                self.result(connection_id, id, meta, result)
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
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<DevicesCancelInviteParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        self.remote_devices.cancel_invite();
        self.publish_global_state(GlobalAreas::DEVICES);
        self.result(connection_id, id, meta, DevicesCancelInviteResult {})
    }

    pub(super) fn handle_devices_remove(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
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
                self.publish_global_state(GlobalAreas::DEVICES);
                self.result(connection_id, id, meta, DevicesRemoveResult {})
            }
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    /// An Own Relay call under the protocol lock. The shared gateway runs these
    /// without it, because they wait on the relay; this path serves gateways with no
    /// relay to wait on, such as the stdio protocol.
    pub(super) fn handle_relay_request(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        method: &str,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let answer = relay_request(self.remote_devices.as_ref(), method, params);
        self.relay_answer(connection_id, id, method, meta, answer)
    }

    /// Answers an Own Relay call. A relay change moves every client's device list.
    pub(super) fn relay_answer(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        method: &str,
        meta: RequestMeta,
        answer: Result<Value, ProtocolError>,
    ) -> GatewayOutcome {
        match answer {
            Ok(result) => {
                if method == DEVICES_SET_RELAY {
                    self.publish_global_state(GlobalAreas::DEVICES);
                }
                self.result(connection_id, id, meta, result)
            }
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    /// Republishes after a change the network edge made, such as a device connecting.
    pub(crate) fn publish_background_device_collection_update(&mut self) {
        self.publish_global_state(GlobalAreas::DEVICES);
    }

    /// Names the client that pairs a device: a Remote Device by its own name, a
    /// same-machine App Shell by the computer's.
    fn pairing_client_label(&self, connection_id: &ConnectionId) -> String {
        self.remote_devices
            .device_name_for_connection(connection_id.as_str())
            .unwrap_or_else(|| self.remote_devices.snapshot().server_name)
    }
}

/// Whether `method` is an Own Relay call, which may wait on the relay.
pub(super) fn is_relay_method(method: &str) -> bool {
    matches!(
        method,
        DEVICES_SET_RELAY
            | DEVICES_RELAY_PEOPLE
            | DEVICES_RELAY_ADD_PERSON
            | DEVICES_RELAY_REVOKE_PERSON
    )
}

/// Runs an Own Relay call. It needs no protocol state, only the devices workflow.
pub(super) fn relay_request(
    devices: &dyn RemoteDevicesWorkflow,
    method: &str,
    params: Value,
) -> Result<Value, ProtocolError> {
    fn read<T: DeserializeOwned>(params: Value) -> Result<T, ProtocolError> {
        serde_json::from_value(params).map_err(responses::invalid_params)
    }
    fn answer<T: serde::Serialize>(result: T) -> Result<Value, ProtocolError> {
        Ok(serde_json::to_value(result).expect("a protocol result serializes"))
    }
    match method {
        DEVICES_SET_RELAY => {
            let params: DevicesSetRelayParams = read(params)?;
            answer(devices.set_relay(params.relay.as_deref())?)
        }
        DEVICES_RELAY_PEOPLE => {
            let _: DevicesRelayPeopleParams = read(params)?;
            answer(devices.relay_people()?)
        }
        DEVICES_RELAY_ADD_PERSON => {
            let params: DevicesRelayAddPersonParams = read(params)?;
            answer(devices.relay_add_person(&params.name, params.can_manage)?)
        }
        DEVICES_RELAY_REVOKE_PERSON => {
            let params: DevicesRelayRevokePersonParams = read(params)?;
            devices.relay_revoke_person(&params.name)?;
            answer(DevicesRelayRevokePersonResult {})
        }
        _ => unreachable!("only Own Relay methods are routed here"),
    }
}
