use super::*;

#[test]
fn a_missing_file_reads_as_no_trust() {
    let root = tempfile::tempdir().unwrap();

    assert_eq!(
        RemoteDeviceStore::new(root.path()).read().unwrap(),
        StoredRemoteDevices::default()
    );
}

#[test]
fn the_identity_and_devices_survive_a_restart() {
    let root = tempfile::tempdir().unwrap();
    let store = RemoteDeviceStore::new(root.path());
    let mut stored = StoredRemoteDevices::default();
    let secret_key = stored.ensure_secret_key();
    stored.devices.push(StoredRemoteDevice {
        device_id: "device".to_string(),
        name: "Phone".to_string(),
        model: None,
        added_at_ms: 5,
        added_by: Some("desk".to_string()),
        last_seen_at_ms: None,
    });

    store.write(&stored).unwrap();

    let reopened = RemoteDeviceStore::new(root.path()).read().unwrap();
    assert_eq!(reopened, stored);
    assert_eq!(
        reopened.secret_key().map(|key| key.public()),
        Some(secret_key.public())
    );
    assert_eq!(stored.ensure_secret_key().public(), secret_key.public());
}

#[cfg(unix)]
#[test]
fn the_file_holding_the_private_key_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let store = RemoteDeviceStore::new(root.path());
    let mut stored = StoredRemoteDevices::default();
    stored.ensure_secret_key();

    store.write(&stored).unwrap();

    let mode = std::fs::metadata(root.path().join(FILE_NAME))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_damaged_file_is_an_error_rather_than_empty_trust() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(FILE_NAME), b"{ not json").unwrap();

    assert!(RemoteDeviceStore::new(root.path()).read().is_err());
}
