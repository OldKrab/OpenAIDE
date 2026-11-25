use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use openaide_remote_client::{NodeHost, OwnRelay, SecretKey};

use super::host::Bridge;
use super::*;

#[derive(Default)]
struct Host {
    key: Mutex<Option<Vec<u8>>>,
    in_use: Mutex<Option<String>>,
    own_relays: Mutex<HashMap<String, String>>,
    broken: AtomicBool,
}

impl RemoteHost for Host {
    fn device_key(&self) -> Result<Option<Vec<u8>>, StorageError> {
        if self.broken.load(Ordering::SeqCst) {
            return Err(StorageError::Unavailable);
        }
        Ok(self.key.lock().unwrap().clone())
    }

    fn save_device_key(&self, key: Vec<u8>) -> Result<(), StorageError> {
        *self.key.lock().unwrap() = Some(key);
        Ok(())
    }

    fn computer_in_use(&self) -> Option<String> {
        self.in_use.lock().unwrap().clone()
    }

    fn relay(&self, _computer: String) -> Option<String> {
        None
    }

    fn save_relay(&self, _computer: String, _relay: String) -> Result<(), StorageError> {
        Ok(())
    }

    fn own_relay(&self, computer: String) -> Result<Option<String>, StorageError> {
        if self.broken.load(Ordering::SeqCst) {
            return Err(StorageError::Unavailable);
        }
        Ok(self.own_relays.lock().unwrap().get(&computer).cloned())
    }

    fn save_own_relay(&self, computer: String, line: Option<String>) -> Result<(), StorageError> {
        let mut own_relays = self.own_relays.lock().unwrap();
        match line {
            Some(line) => own_relays.insert(computer, line),
            None => own_relays.remove(&computer),
        };
        Ok(())
    }

    fn network(&self) -> NetworkKind {
        network_kind(true, false, false, false, true)
    }

    fn log(&self, _event: String, _fields: String) {}
}

#[test]
fn a_key_kept_by_the_shell_reads_back_as_the_same_key() {
    let host = Arc::new(Host::default());
    let bridge = Bridge(host.clone());
    let key = SecretKey::generate();
    bridge.save_device_key(&key).unwrap();

    assert_eq!(host.key.lock().unwrap().as_ref().unwrap().len(), 32);
    assert_eq!(bridge.device_key().unwrap().unwrap().public(), key.public());
}

#[test]
fn a_key_saved_by_an_earlier_app_keeps_its_identity() {
    // Earlier versions stored iroh-android's `SecretKey.toBytes()`: the Ed25519 seed.
    // This public key is what iroh-android 1.1.0 derived from the same seed, so every
    // computer that trusts such a phone still knows it.
    let seed: Vec<u8> = (0..32u8).map(|index| index * 7 + 1).collect();
    let host = Arc::new(Host::default());
    *host.key.lock().unwrap() = Some(seed);
    assert_eq!(
        Bridge(host)
            .device_key()
            .unwrap()
            .unwrap()
            .public()
            .to_string(),
        "e4030998cfd5ad1723c169f956aa0b9eb8619b5992bd612c2af428ebc79f8df0"
    );
}

#[test]
fn a_damaged_or_unreadable_key_is_a_storage_failure_not_a_new_key() {
    let host = Arc::new(Host::default());
    *host.key.lock().unwrap() = Some(vec![7; 31]);
    assert!(Bridge(host.clone()).device_key().is_err());
    host.broken.store(true, Ordering::SeqCst);
    assert!(Bridge(host).device_key().is_err());
}

#[test]
fn the_computer_in_use_is_read_from_the_stored_hex_id() {
    let host = Arc::new(Host::default());
    let computer = SecretKey::generate().public();
    *host.in_use.lock().unwrap() = Some(computer.to_string());
    assert_eq!(Bridge(host.clone()).computer_in_use(), Some(computer));
    *host.in_use.lock().unwrap() = Some("not a key".to_string());
    assert_eq!(Bridge(host).computer_in_use(), None);
}

#[test]
fn a_code_names_its_computer_and_a_refusal_says_why() {
    let server = SecretKey::generate().public();
    let code = InviteCode {
        server,
        secret: [3; 16],
        relay: None,
    }
    .encode();
    let invite = parse_invite(format!(" {} ", code.to_lowercase())).unwrap();
    assert_eq!(invite.server_id(), server.to_string());
    assert!(invite.is_from(server.to_string()));
    assert!(!invite.is_from(SecretKey::generate().public().to_string()));

    assert_eq!(
        parse_invite("hello".into()).unwrap_err(),
        CodeError::NotAPairingCode
    );
    assert_eq!(
        parse_invite("OAI1ABC".into()).unwrap_err(),
        CodeError::Malformed
    );
}

#[test]
fn a_code_that_names_an_own_relay_is_read_with_its_relay() {
    let server = SecretKey::generate().public();
    let relay = OwnRelay::parse("https://relay.example.com/?token=t").unwrap();
    let code = InviteCode {
        server,
        secret: [3; 16],
        relay: Some(relay.clone()),
    }
    .encode();
    assert!(code.starts_with("OAI2"));

    let invite = parse_invite(code).unwrap();
    assert_eq!(invite.server_id(), server.to_string());
    assert_eq!(invite.0.relay, Some(relay));
    // The relay's password stays out of anything printed.
    assert!(!format!("{invite:?}").contains("token"));
}

#[test]
fn the_own_relay_line_crosses_to_the_shell_and_back_unchanged() {
    let host = Arc::new(Host::default());
    let bridge = Bridge(host.clone());
    let computer = SecretKey::generate().public();
    let line = "https://relay.example.com/?token=t";

    bridge.save_own_relay(&computer, Some(line)).unwrap();
    assert_eq!(bridge.own_relay(&computer).unwrap().as_deref(), Some(line));
    bridge.save_own_relay(&computer, None).unwrap();
    assert_eq!(bridge.own_relay(&computer).unwrap(), None);
    // Unreadable storage is a failure, never a computer without an Own Relay.
    host.broken.store(true, Ordering::SeqCst);
    assert!(bridge.own_relay(&computer).is_err());
}

#[test]
fn the_network_kind_crosses_to_the_node_unchanged() {
    let kind = network_kind(false, true, false, true, true);
    assert_eq!(kind.transport, Transport::Cellular);
    assert!(kind.vpn);
    assert_eq!(NetworkKind::from(remote::NetworkKind::from(kind)), kind);
}

#[test]
fn a_cancelled_call_stops_its_work() {
    let (started, started_rx) = std::sync::mpsc::channel();
    let (_never, held) = tokio::sync::oneshot::channel::<()>();
    let (dropped, dropped_rx) = std::sync::mpsc::channel::<()>();
    struct Signal(std::sync::mpsc::Sender<()>);
    impl Drop for Signal {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let call = on_runtime(async move {
        let _signal = Signal(dropped);
        let _ = started.send(());
        let _ = held.await;
    });
    let mut call = Box::pin(call);
    let waker = std::task::Waker::noop();
    assert!(call
        .as_mut()
        .poll(&mut std::task::Context::from_waker(waker))
        .is_pending());
    started_rx.recv().unwrap();
    // The coroutine was cancelled: uniffi drops the call's future.
    drop(call);
    dropped_rx.recv().unwrap();
}
