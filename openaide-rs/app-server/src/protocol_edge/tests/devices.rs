use iroh::SecretKey;
use openaide_app_server_protocol::devices::{DevicesCreateInviteParams, DevicesRemoveParams};
use openaide_app_server_protocol::events::AppServerEventPayload;
use openaide_app_server_protocol::methods::{DEVICES_CREATE_INVITE, DEVICES_REMOVE};
use openaide_remote_core::InviteCode;
use serde_json::json;

use super::*;
use crate::remote_devices::RemoteDevices;

fn gateway_with_devices() -> (RpcGateway, tempfile::TempDir) {
    let (gateway, _devices, root) = gateway_and_devices();
    (gateway, root)
}

/// The gateway, and the device collection behind it for the network edge's side of a test.
fn gateway_and_devices() -> (RpcGateway, RemoteDevices, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let devices = RemoteDevices::open(root.path());
    // Stands in for the network edge, which these tests do not exercise.
    devices.attach_edge(|_command| {});
    (
        gateway().with_remote_devices(devices.clone()),
        devices,
        root,
    )
}

fn subscribe_to_devices(gateway: &mut RpcGateway, connection_id: &ConnectionId) -> Value {
    response_value(gateway.handle_inbound(
        connection_id.clone(),
        request(
            "subscribe",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Devices,
                known_revision: None,
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
fn a_device_paired_by_invite_and_then_removed_reaches_subscribed_clients() {
    let (mut gateway, devices, _root) = gateway_and_devices();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());
    subscribe_to_devices(&mut gateway, &connection_id);
    let shown = response_value(gateway.handle_inbound(
        connection_id.clone(),
        request("3", DEVICES_CREATE_INVITE, DevicesCreateInviteParams {}),
        AppServerTime(3),
    ));
    let invite = InviteCode::decode(shown["result"]["code"].as_str().unwrap()).unwrap();
    gateway.drain_event_deliveries_for_connection(&connection_id);

    // The network edge spends the invite for a new device.
    let tablet = SecretKey::from_bytes(&[3; 32]).public();
    devices
        .redeem_invite(&tablet, &invite.secret, "Tablet", None, 4)
        .unwrap();
    let published = gateway.publish_background_device_collection_update(AppServerTime(4));

    let [delivery] = published.as_slice() else {
        panic!("expected one device collection event, got {published:?}");
    };
    let AppServerEventPayload::DeviceCollectionUpdated { devices } = &delivery.event.payload else {
        panic!("unexpected payload {:?}", delivery.event.payload);
    };
    assert_eq!(devices.devices.len(), 1);
    assert_eq!(devices.devices[0].device_id, tablet.to_string());
    assert_eq!(devices.devices[0].name, "Tablet");
    assert_eq!(devices.server_key, Some(invite.server.to_string()));
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
                device_id: tablet.to_string(),
            },
        ),
        AppServerTime(5),
    ));
    let [delivery] = events.as_slice() else {
        panic!("expected one device collection event, got {events:?}");
    };
    // Removing the last device forgets the key, so a later code carries a new one.
    assert!(matches!(
        &delivery.event.payload,
        AppServerEventPayload::DeviceCollectionUpdated { devices }
            if devices.devices.is_empty() && devices.server_key.is_none()
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
