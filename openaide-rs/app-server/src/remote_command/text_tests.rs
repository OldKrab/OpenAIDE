use openaide_app_server_protocol::devices::{
    DeviceCollectionSnapshot, RemoteAccessState, RemoteDeviceConnection, RemoteDevicePath,
    RemoteDeviceSummary,
};

use super::*;

fn device(id: &str, name: &str, added_at_ms: u64, connected: bool) -> RemoteDeviceSummary {
    RemoteDeviceSummary {
        device_id: id.to_string(),
        name: name.to_string(),
        model: None,
        added_at_ms,
        added_by: None,
        last_seen_at_ms: None,
        connection: connected.then_some(RemoteDeviceConnection {
            path: RemoteDevicePath::Direct,
            address: None,
        }),
    }
}

fn list(devices: Vec<RemoteDeviceSummary>) -> DeviceCollectionSnapshot {
    DeviceCollectionSnapshot {
        remote_access: RemoteAccessState::On,
        server_name: "studio".to_string(),
        devices,
    }
}

#[test]
fn numbers_devices_from_the_oldest_pairing() {
    let snapshot = list(vec![
        device("b", "Phone", 20, true),
        device("a", "Laptop", 10, false),
    ]);
    assert_eq!(
        device_list(&snapshot),
        "  1  Laptop  ·  not connected\n  2  Phone  ·  connected\n"
    );
    assert_eq!(
        device_list(&list(Vec::new())),
        "No devices are paired with this computer.\n"
    );
}

#[test]
fn names_what_changed_between_two_lists() {
    let before = list(vec![
        device("a", "Laptop", 10, false),
        device("b", "Phone", 20, true),
    ]);
    let after = list(vec![
        device("a", "Laptop", 10, true),
        device("c", "Tablet", 30, true),
    ]);
    assert_eq!(
        changes(&before, &after),
        vec![
            DeviceChange::Connected("Laptop".into()),
            DeviceChange::Paired("Tablet".into()),
            DeviceChange::Connected("Tablet".into()),
            DeviceChange::Removed("Phone".into()),
        ]
    );
    assert!(changes(&before, &before).is_empty());
    let left = list(vec![
        device("a", "Laptop", 10, false),
        device("b", "Phone", 20, false),
    ]);
    assert_eq!(
        changes(&before, &left),
        vec![DeviceChange::Left("Phone".into())]
    );
}

#[test]
fn a_device_name_cannot_steer_the_terminal() {
    let mut hostile = device("a", "\u{1b}[2JPhone\r\n", 1, false);
    hostile.model = Some("Pixel\u{7}".to_string());
    assert_eq!(label(&hostile), "[2JPhone (Pixel)");
    assert_eq!(label(&device("a", " \n", 1, false)), "Device");
}

#[test]
fn words_the_stop_warning_and_the_time_left() {
    assert!(stop_warning(1).starts_with("1 task is running."));
    assert!(stop_warning(3).starts_with("3 tasks are running."));
    assert_eq!(minutes_left(600_000, 0), 10);
    assert_eq!(minutes_left(600_000, 590_000), 1);
    assert_eq!(minutes_left(1, 5), 0);
}
