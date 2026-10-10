use openaide_app_server_protocol::devices::{
    DevicesApproveJoinRequestParams, DevicesCreateInviteParams, DevicesPreviewJoinRequestParams,
    DevicesRemoveParams,
};
use openaide_app_server_protocol::events::AppServerEventPayload;
use openaide_app_server_protocol::methods::{
    DEVICES_APPROVE_JOIN_REQUEST, DEVICES_CREATE_INVITE, DEVICES_PREVIEW_JOIN_REQUEST,
    DEVICES_REMOVE,
};
use serde_json::json;

use super::*;
use crate::remote_devices::RemoteDevices;

/// A join-request Pairing Code for a fixed device key, as a new device would show it.
const JOIN_CODE: &str = "OAJ15VESRRRI2HBMN2XJAM4JAWMVMEUVSJZ2LRR7SNRWYFDBJLEHG7IQMVDBMJWGK5AA";
const JOIN_DEVICE_ID: &str = "ed4928c628d1c2c6eae90338905995612959273a5c63f93636c14614ac8737d1";

fn gateway_with_devices() -> (RpcGateway, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let devices = RemoteDevices::open(root.path());
    // Stands in for the network edge, which these tests do not exercise.
    devices.attach_edge(|_command| {});
    (gateway().with_remote_devices(devices), root)
}

fn subscribe_to_devices(gateway: &mut RpcGateway, connection_id: &ConnectionId) -> Value {
    response_value(gateway.handle_inbound(
        connection_id.clone(),
        request(
            "subscribe",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Devices,
            },
        ),
        AppServerTime(2),
    ))
}

#[test]
fn the_device_collection_is_a_subscription_every_client_can_read() {
    let (mut gateway, _root) = gateway_with_devices();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());

    let value = subscribe_to_devices(&mut gateway, &connection_id);

    assert_eq!(value["result"]["snapshot"]["kind"], json!("devices"));
    assert_eq!(
        value["result"]["snapshot"]["devices"]["remoteAccess"],
        json!("off")
    );
}

#[test]
fn an_invite_is_a_pairing_code_only_its_creator_receives() {
    let (mut gateway, _root) = gateway_with_devices();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());

    let value = response_value(gateway.handle_inbound(
        connection_id,
        request("2", DEVICES_CREATE_INVITE, DevicesCreateInviteParams {}),
        AppServerTime(5),
    ));

    assert!(value["result"]["code"]
        .as_str()
        .is_some_and(|code| code.starts_with("OAI1")));
    assert!(value["result"]["expiresAtMs"]
        .as_u64()
        .is_some_and(|at| at > 5));
}

#[test]
fn approving_a_join_request_tells_subscribed_clients_about_the_new_device() {
    let (mut gateway, _root) = gateway_with_devices();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());
    subscribe_to_devices(&mut gateway, &connection_id);

    let preview = response_value(gateway.handle_inbound(
        connection_id.clone(),
        request(
            "3",
            DEVICES_PREVIEW_JOIN_REQUEST,
            DevicesPreviewJoinRequestParams {
                code: JOIN_CODE.to_string(),
            },
        ),
        AppServerTime(3),
    ));
    assert_eq!(preview["result"]["name"], json!("Tablet"));
    assert_eq!(preview["result"]["deviceId"], json!(JOIN_DEVICE_ID));
    assert_eq!(preview["result"]["alreadyTrusted"], json!(false));

    let events = response_events(gateway.handle_inbound(
        connection_id.clone(),
        request(
            "4",
            DEVICES_APPROVE_JOIN_REQUEST,
            DevicesApproveJoinRequestParams {
                code: JOIN_CODE.to_string(),
            },
        ),
        AppServerTime(4),
    ));

    let [delivery] = events.as_slice() else {
        panic!("expected one device collection event, got {events:?}");
    };
    let AppServerEventPayload::DeviceCollectionUpdated { devices } = &delivery.event.payload else {
        panic!("unexpected payload {:?}", delivery.event.payload);
    };
    assert_eq!(devices.devices.len(), 1);
    assert_eq!(devices.devices[0].device_id, JOIN_DEVICE_ID);
    assert_eq!(devices.devices[0].name, "Tablet");
    // A same-machine client pairs in the computer's name.
    assert_eq!(
        devices.devices[0].added_by.as_deref(),
        Some(devices.server_name.as_str())
    );

    let events = response_events(gateway.handle_inbound(
        connection_id,
        request(
            "5",
            DEVICES_REMOVE,
            DevicesRemoveParams {
                device_id: JOIN_DEVICE_ID.to_string(),
            },
        ),
        AppServerTime(5),
    ));
    let [delivery] = events.as_slice() else {
        panic!("expected one device collection event, got {events:?}");
    };
    assert!(matches!(
        &delivery.event.payload,
        AppServerEventPayload::DeviceCollectionUpdated { devices } if devices.devices.is_empty()
    ));
}

#[test]
fn a_change_made_by_the_network_edge_is_queued_for_subscribers() {
    let (mut gateway, _root) = gateway_with_devices();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());
    subscribe_to_devices(&mut gateway, &connection_id);

    let published = gateway.publish_background_device_collection_update(AppServerTime(3));

    assert_eq!(published.len(), 1);
    assert_eq!(
        gateway.drain_event_deliveries_for_connection(&connection_id),
        published
    );
}

#[test]
fn pairing_is_unavailable_without_a_network_edge() {
    let mut gateway = gateway();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());

    let error = response_error(gateway.handle_inbound(
        connection_id,
        request("2", DEVICES_CREATE_INVITE, DevicesCreateInviteParams {}),
        AppServerTime(5),
    ));

    assert_eq!(error.error.code, ProtocolErrorCode::CapabilityUnavailable);
}
