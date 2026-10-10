use std::sync::mpsc;

use super::*;

struct Fixture {
    _root: tempfile::TempDir,
    devices: RemoteDevices,
    commands: mpsc::Receiver<EdgeCommand>,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let devices = RemoteDevices::open(root.path());
    let (sender, commands) = mpsc::channel();
    devices.attach_edge(move |command| {
        let _ = sender.send(command);
    });
    assert_eq!(commands.try_recv(), Ok(EdgeCommand::Reconcile));
    Fixture {
        _root: root,
        devices,
        commands,
    }
}

fn device(seed: u8) -> PublicKey {
    SecretKey::from_bytes(&[seed; 32]).public()
}

fn invite_secret(devices: &RemoteDevices, now_ms: u64) -> [u8; INVITE_SECRET_LEN] {
    let invite = devices.create_invite("desk".to_string(), now_ms).unwrap();
    InviteCode::decode(&invite.code).unwrap().secret
}

fn join_code(seed: u8, name: &str) -> String {
    JoinCode {
        device: device(seed),
        name: name.to_string(),
        model: Some("Model X".to_string()),
    }
    .encode()
}

#[test]
fn remote_access_is_unused_until_the_first_invite() {
    let fixture = fixture();

    assert!(fixture.devices.endpoint_secret_key().is_none());
    assert_eq!(
        fixture.devices.snapshot().remote_access,
        RemoteAccessState::Off
    );

    let invite = fixture
        .devices
        .create_invite("desk".to_string(), 1_000)
        .unwrap();

    let code = InviteCode::decode(&invite.code).unwrap();
    assert_eq!(
        fixture
            .devices
            .endpoint_secret_key()
            .map(|key| key.public()),
        Some(code.server)
    );
    assert_eq!(invite.expires_at_ms, 1_000 + INVITE_LIFETIME_MS);
    assert_eq!(fixture.commands.try_recv(), Ok(EdgeCommand::Reconcile));
}

#[test]
fn a_device_presenting_the_invite_secret_is_trusted_once() {
    let fixture = fixture();
    let secret = invite_secret(&fixture.devices, 1_000);

    assert_eq!(
        fixture
            .devices
            .redeem_invite(&device(1), &secret, " Phone ", Some("Model X"), 2_000),
        Ok(())
    );

    assert!(fixture.devices.is_trusted(&device(1)));
    let snapshot = fixture.devices.snapshot();
    assert_eq!(snapshot.devices.len(), 1);
    assert_eq!(snapshot.devices[0].name, "Phone");
    assert_eq!(snapshot.devices[0].model.as_deref(), Some("Model X"));
    assert_eq!(snapshot.devices[0].added_by.as_deref(), Some("desk"));
    assert_eq!(snapshot.devices[0].added_at_ms, 2_000);
    // The invite is spent: a second device cannot reuse it.
    assert_eq!(
        fixture
            .devices
            .redeem_invite(&device(2), &secret, "Tablet", None, 2_001),
        Err(InviteRejection::NoInvite)
    );
    assert!(!fixture.devices.is_trusted(&device(2)));
}

#[test]
fn a_wrong_or_late_secret_trusts_nobody() {
    let fixture = fixture();
    let secret = invite_secret(&fixture.devices, 1_000);

    assert_eq!(
        fixture
            .devices
            .redeem_invite(&device(1), &[0; INVITE_SECRET_LEN], "Phone", None, 2_000),
        Err(InviteRejection::WrongSecret)
    );
    assert_eq!(
        fixture.devices.redeem_invite(
            &device(1),
            &secret,
            "Phone",
            None,
            1_000 + INVITE_LIFETIME_MS
        ),
        Err(InviteRejection::Expired)
    );
    assert!(!fixture.devices.is_trusted(&device(1)));
}

#[test]
fn a_new_invite_replaces_the_previous_one() {
    let fixture = fixture();
    let first = invite_secret(&fixture.devices, 1_000);
    let second = invite_secret(&fixture.devices, 1_001);

    assert_eq!(
        fixture
            .devices
            .redeem_invite(&device(1), &first, "Phone", None, 1_002),
        Err(InviteRejection::WrongSecret)
    );
    assert_eq!(
        fixture
            .devices
            .redeem_invite(&device(1), &second, "Phone", None, 1_003),
        Ok(())
    );
}

#[test]
fn cancelling_an_unused_invite_drops_the_identity() {
    let fixture = fixture();
    let secret = invite_secret(&fixture.devices, 1_000);

    fixture.devices.cancel_invite();

    assert!(fixture.devices.endpoint_secret_key().is_none());
    assert_eq!(
        fixture
            .devices
            .redeem_invite(&device(1), &secret, "Phone", None, 1_001),
        Err(InviteRejection::NoInvite)
    );
}

#[test]
fn a_join_request_is_previewed_before_it_is_trusted() {
    let fixture = fixture();
    let code = join_code(3, "Tablet");

    let preview = fixture.devices.preview_join_request(&code).unwrap();

    assert_eq!(preview.device_id, device(3).to_string());
    assert_eq!(preview.name, "Tablet");
    assert_eq!(preview.model.as_deref(), Some("Model X"));
    assert!(!preview.already_trusted);
    assert!(!fixture.devices.is_trusted(&device(3)));
}

#[test]
fn approving_a_join_request_trusts_the_device_and_announces_it() {
    let fixture = fixture();

    fixture
        .devices
        .approve_join_request(&join_code(3, "Tablet"), "Phone".to_string(), 4_000)
        .unwrap();

    assert!(fixture.devices.is_trusted(&device(3)));
    assert!(fixture.devices.endpoint_secret_key().is_some());
    assert_eq!(fixture.commands.try_recv(), Ok(EdgeCommand::Reconcile));
    assert_eq!(
        fixture.commands.try_recv(),
        Ok(EdgeCommand::AnnounceTrust(device(3)))
    );
    assert_eq!(
        fixture.devices.snapshot().devices[0].added_by.as_deref(),
        Some("Phone")
    );
}

#[test]
fn an_invite_code_is_refused_as_a_join_request() {
    let fixture = fixture();
    let invite = fixture
        .devices
        .create_invite("desk".to_string(), 1)
        .unwrap();

    let error = fixture
        .devices
        .approve_join_request(&invite.code, "desk".to_string(), 2)
        .unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::ValidationFailed);
}

#[test]
fn removing_a_device_revokes_it_and_closes_its_connections() {
    let fixture = fixture();
    fixture
        .devices
        .approve_join_request(&join_code(3, "Tablet"), "desk".to_string(), 4_000)
        .unwrap();
    fixture
        .devices
        .approve_join_request(&join_code(4, "Phone"), "desk".to_string(), 4_001)
        .unwrap();
    while fixture.commands.try_recv().is_ok() {}

    fixture.devices.remove(&device(3).to_string()).unwrap();

    assert!(!fixture.devices.is_trusted(&device(3)));
    assert!(fixture.devices.is_trusted(&device(4)));
    assert_eq!(
        fixture.commands.try_recv(),
        Ok(EdgeCommand::Disconnect(device(3)))
    );
    assert_eq!(fixture.commands.try_recv(), Ok(EdgeCommand::Reconcile));
    assert_eq!(
        fixture
            .devices
            .remove(&device(3).to_string())
            .unwrap_err()
            .code,
        ProtocolErrorCode::NotFound
    );
}

#[test]
fn removing_the_last_device_turns_remote_access_off_with_a_fresh_identity_next_time() {
    let fixture = fixture();
    let first = invite_secret(&fixture.devices, 1);
    let first_identity = fixture.devices.endpoint_secret_key().unwrap().public();
    fixture
        .devices
        .redeem_invite(&device(1), &first, "Phone", None, 2)
        .unwrap();

    fixture.devices.remove(&device(1).to_string()).unwrap();

    assert!(fixture.devices.endpoint_secret_key().is_none());
    invite_secret(&fixture.devices, 3);
    assert_ne!(
        fixture.devices.endpoint_secret_key().unwrap().public(),
        first_identity
    );
}

#[test]
fn trust_survives_a_restart_and_pending_invites_do_not() {
    let root = tempfile::tempdir().unwrap();
    let devices = RemoteDevices::open(root.path());
    devices.attach_edge(|_| {});
    let secret = invite_secret(&devices, 1);
    devices
        .redeem_invite(&device(1), &secret, "Phone", None, 2)
        .unwrap();
    invite_secret(&devices, 3);

    let reopened = RemoteDevices::open(root.path());

    assert!(reopened.is_trusted(&device(1)));
    assert!(reopened.endpoint_secret_key().is_some());
    assert_eq!(
        reopened.redeem_invite(&device(2), &[0; INVITE_SECRET_LEN], "Tablet", None, 4),
        Err(InviteRejection::NoInvite)
    );
}

#[test]
fn a_damaged_trust_file_leaves_remote_access_off() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("remote-devices.json"), b"{ not json").unwrap();

    let devices = RemoteDevices::open(root.path());

    assert!(devices.endpoint_secret_key().is_none());
    assert!(devices.snapshot().devices.is_empty());
}

#[test]
fn pairing_needs_a_network_edge() {
    let root = tempfile::tempdir().unwrap();
    let devices = RemoteDevices::open(root.path());

    let error = devices.create_invite("desk".to_string(), 1).unwrap_err();

    assert_eq!(error.code, ProtocolErrorCode::CapabilityUnavailable);
}

#[test]
fn connection_state_follows_the_devices_open_connections() {
    let fixture = fixture();
    let secret = invite_secret(&fixture.devices, 1);
    fixture
        .devices
        .redeem_invite(&device(1), &secret, "Phone", None, 2)
        .unwrap();
    let (changes, changed) = mpsc::channel();
    fixture.devices.on_changed(move || {
        let _ = changes.send(());
    });
    let relayed = RemoteDeviceConnection {
        path: openaide_app_server_protocol::devices::RemoteDevicePath::Relayed,
        address: None,
    };
    let direct = RemoteDeviceConnection {
        path: openaide_app_server_protocol::devices::RemoteDevicePath::Direct,
        address: Some("192.0.2.7".to_string()),
    };

    fixture
        .devices
        .device_connected(&device(1), relayed.clone());
    fixture
        .devices
        .device_connected(&device(1), relayed.clone());
    fixture.devices.device_path_changed(&device(1), relayed);
    fixture
        .devices
        .device_path_changed(&device(1), direct.clone());
    fixture
        .devices
        .note_protocol_connection(&device(1), "conn-1");

    assert_eq!(changed.try_iter().count(), 3);
    assert_eq!(
        fixture.devices.snapshot().devices[0].connection,
        Some(direct)
    );
    assert_eq!(
        fixture
            .devices
            .device_name_for_connection("conn-1")
            .as_deref(),
        Some("Phone")
    );

    // One of two connections closing is a network change, not a departure.
    fixture.devices.device_disconnected(&device(1), 9_000);
    assert!(fixture.devices.snapshot().devices[0].connection.is_some());
    fixture.devices.device_disconnected(&device(1), 9_500);

    let summary = &fixture.devices.snapshot().devices[0];
    assert_eq!(summary.connection, None);
    assert_eq!(summary.last_seen_at_ms, Some(9_500));
    assert_eq!(fixture.devices.device_name_for_connection("conn-1"), None);
}
