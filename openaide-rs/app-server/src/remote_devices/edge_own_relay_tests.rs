//! The edge on an Own Relay (ADR-0085): a relay that admits only holders of a password.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use iroh_relay::server::{Access, AccessControl, ClientRequest};
use openaide_remote_client::{OwnRelay, RemoteClient};

use super::*;

const PASSWORD: &str = "right-password";

/// Admits a client that presents [`PASSWORD`], and counts whom it admitted and
/// turned away. Clones share the counts.
#[derive(Debug, Default, Clone)]
struct PasswordGate {
    admitted: Arc<AtomicU32>,
    refused: Arc<AtomicU32>,
}

impl PasswordGate {
    fn admitted(&self) -> u32 {
        self.admitted.load(Ordering::SeqCst)
    }

    fn refused(&self) -> u32 {
        self.refused.load(Ordering::SeqCst)
    }
}

impl AccessControl for PasswordGate {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        if request.auth_token().as_deref() == Some(PASSWORD) {
            self.admitted.fetch_add(1, Ordering::SeqCst);
            Access::Allow
        } else {
            self.refused.fetch_add(1, Ordering::SeqCst);
            Access::Deny { reason: None }
        }
    }
}

/// A device that knows no relay until its pairing code names one, as a device on
/// the public network knows none but the public relays. It has no IP transport, so
/// on loopback it cannot find a direct path and every connection stays relayed.
fn bare_device(fixture: &Fixture, seed: u8) -> Endpoint {
    fixture.run(async move {
        Network::Relay(iroh::RelayMap::empty())
            .builder()
            .secret_key(SecretKey::from_bytes(&[seed; 32]))
            .clear_ip_transports()
            .bind()
            .await
            .unwrap()
    })
}

#[test]
fn a_device_pairs_and_connects_through_the_own_relay_its_code_names() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let gate = PasswordGate::default();
    let (_, relay_url, _relay) = runtime
        .block_on(iroh::test_utils::run_relay_server_with_access(
            true,
            Arc::new(gate.clone()),
        ))
        .unwrap();
    // The edge starts on a test network with no relay of its own, so it can reach the
    // relay only through the Own Relay set below.
    let fixture = fixture_on(Network::Relay(iroh::RelayMap::empty()));
    let line = format!("{relay_url}?token={PASSWORD}");
    let changed = fixture.devices.set_relay(Some(&line)).unwrap();
    assert_eq!(changed.removed_devices, 0);
    let host = OwnRelay::parse(&line).unwrap().host().to_string();
    let shown_relay = fixture.devices.snapshot().own_relay.unwrap();
    assert_eq!(shown_relay.host, host);
    assert!(!shown_relay.can_manage);

    let shown = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let invite = InviteCode::decode(&shown.code).unwrap();
    assert_eq!(invite.relay, OwnRelay::parse(&line).ok());

    let device = bare_device(&fixture, 31);
    let mut log = EdgeLog::capture();
    let answer = fixture.run({
        let device = device.clone();
        async move {
            let paired = openaide_remote_client::redeem(&device, &invite, "Laptop", None, &quiet())
                .await
                .unwrap();
            assert_eq!(paired.relay, invite.relay);
            let client = RemoteClient::new(device, paired.id, quiet()).on_relay(paired.relay);
            let connection = client.connection().await.unwrap();
            request(&connection, "POST /app HTTP/1.1\r\n\r\n").await
        }
    });

    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert_eq!(
        log.fields("remote_device_connected", &device)["route"],
        "relayed"
    );
    assert!(gate.admitted() >= 2, "edge and device");
    assert_eq!(gate.refused(), 0);
}

#[test]
fn a_wrong_password_is_turned_away_by_the_relay() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let gate = PasswordGate::default();
    let (_, relay_url, _relay) = runtime
        .block_on(iroh::test_utils::run_relay_server_with_access(
            true,
            Arc::new(gate.clone()),
        ))
        .unwrap();
    let fixture = fixture_on(Network::Relay(iroh::RelayMap::empty()));
    fixture
        .devices
        .set_relay(Some(&format!("{relay_url}?token={PASSWORD}")))
        .unwrap();
    let shown = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let mut invite = InviteCode::decode(&shown.code).unwrap();
    invite.relay = OwnRelay::parse(&format!("{relay_url}?token=guessed")).ok();

    let device = bare_device(&fixture, 32);
    let attempt = fixture.runtime.spawn(async move {
        openaide_remote_client::redeem(&device, &invite, "Laptop", None, &quiet()).await
    });
    wait_until("the relay turns the device away", || gate.refused() >= 1);
    // Pairing would wait out its own timeout; it is enough that it has not succeeded.
    attempt.abort();
    let attempt = fixture.run(attempt);

    assert!(
        !matches!(attempt, Ok(Ok(_))),
        "paired with a wrong password"
    );
    assert!(fixture.devices.snapshot().devices.is_empty());
}

#[test]
fn changing_the_relay_removes_every_device_and_the_identity() {
    let fixture = fixture();
    let shown = fixture
        .devices
        .create_invite("desk".to_string(), AppServerTime::now().0)
        .unwrap();
    fixture.wait_for_access(RemoteAccessState::On);
    let invite = InviteCode::decode(&shown.code).unwrap();
    assert_eq!(invite.relay, None);
    fixture
        .devices
        .redeem_invite(
            &SecretKey::from_bytes(&[33; 32]).public(),
            &invite.secret,
            "Phone",
            None,
            AppServerTime::now().0,
        )
        .unwrap();

    let refused = fixture
        .devices
        .set_relay(Some("https://relay.example.com/"));
    assert!(refused.is_err(), "a line without a password");
    assert_eq!(fixture.devices.snapshot().devices.len(), 1);

    let changed = fixture
        .devices
        .set_relay(Some("https://relay.example.com/?token=abc"))
        .unwrap();
    assert_eq!(changed.removed_devices, 1);
    let snapshot = fixture.devices.snapshot();
    assert!(snapshot.devices.is_empty());
    assert_eq!(snapshot.own_relay.unwrap().host, "relay.example.com");
    // With no device left the identity is gone, so the edge stops until a new invite.
    fixture.wait_for_access(RemoteAccessState::Off);
    assert!(fixture.devices.endpoint_secret_key().is_none());

    let same = fixture
        .devices
        .set_relay(Some(" https://relay.example.com/?token=abc "))
        .unwrap();
    assert_eq!(same.removed_devices, 0, "the same line changes nothing");
    fixture.devices.set_relay(None).unwrap();
    assert_eq!(fixture.devices.snapshot().own_relay, None);
}
