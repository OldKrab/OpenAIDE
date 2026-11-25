//! The node against App Servers on the in-memory network: each fake computer
//! answers every request of a trusted device and closes a connection from any
//! other key as the App Server does.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU32;
use std::sync::mpsc;
use std::time::Duration;

use iroh::address_lookup::MemoryLookup;
use iroh_relay::server::{Access, AccessControl, ClientRequest};
use openaide_remote_core::{close, PairingResult, PAIRING_ALPN, PAIRING_MESSAGE_LIMIT};

use super::*;

/// The suite's watchdog: expires only when a test is already failing.
const WATCHDOG: Duration = Duration::from_secs(30);

/// A computer whose App Server trusts the keys in `trusted`.
struct Computer {
    endpoint: Endpoint,
    trusted: Arc<Mutex<HashSet<PublicKey>>>,
}

impl Computer {
    async fn start(network: &Network, alpn: Vec<u8>) -> Self {
        Self::start_with(network, vec![alpn]).await
    }

    /// A computer that also spends any invite, trusting every device that pairs.
    async fn start_pairing(network: &Network) -> Self {
        Self::start_with(network, vec![application_alpn(), PAIRING_ALPN.to_vec()]).await
    }

    async fn start_with(network: &Network, alpns: Vec<Vec<u8>>) -> Self {
        let endpoint = network
            .builder()
            .secret_key(SecretKey::generate())
            .alpns(alpns)
            .bind()
            .await
            .unwrap();
        network.on_bound(&endpoint);
        let trusted = Arc::new(Mutex::new(HashSet::new()));
        let accepting = endpoint.clone();
        let trust = trusted.clone();
        tokio::spawn(async move {
            while let Some(incoming) = accepting.accept().await {
                let trust = trust.clone();
                tokio::spawn(async move {
                    let Ok(connection) = incoming.await else {
                        return;
                    };
                    if connection.alpn() == PAIRING_ALPN {
                        spend_invite(&connection, &trust).await;
                        return;
                    }
                    if !lock(&trust).contains(&connection.remote_id()) {
                        connection.close(close::UNTRUSTED.into(), b"untrusted");
                        return;
                    }
                    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                        tokio::spawn(async move {
                            let _ = recv.read_to_end(4096).await;
                            let _ = send.write_all(b"HTTP/1.1 204 No Content\r\n\r\n").await;
                            let _ = send.finish();
                        });
                    }
                });
            }
        });
        Self { endpoint, trusted }
    }

    fn id(&self) -> PublicKey {
        self.endpoint.id()
    }

    fn trust(&self, device: PublicKey) {
        lock(&self.trusted).insert(device);
    }
}

/// Answers one invite as the App Server does when its secret is right.
async fn spend_invite(connection: &Connection, trusted: &Mutex<HashSet<PublicKey>>) {
    let Ok((mut send, mut recv)) = connection.accept_bi().await else {
        return;
    };
    if recv.read_to_end(PAIRING_MESSAGE_LIMIT).await.is_err() {
        return;
    }
    lock(trusted).insert(connection.remote_id());
    let answer = PairingResult {
        trusted: true,
        server_name: Some("Desk".to_string()),
    };
    let _ = send.write_all(&serde_json::to_vec(&answer).unwrap()).await;
    let _ = send.finish();
    // The device closes the connection once it has read the answer.
    let _ = connection.closed().await;
}

/// Holds a dial for `computer` at its relay lookup, after the dial has its
/// endpoint, until the test releases it.
struct Gate {
    computer: PublicKey,
    entered: tokio::sync::oneshot::Sender<()>,
    release: mpsc::Receiver<()>,
}

#[derive(Default)]
struct Host {
    key: Mutex<Option<SecretKey>>,
    saved_keys: Mutex<usize>,
    in_use: Mutex<Option<PublicKey>>,
    relays: Mutex<HashMap<PublicKey, String>>,
    own_relays: Mutex<HashMap<PublicKey, String>>,
    gate: Mutex<Option<Gate>>,
}

impl NodeHost for Host {
    fn device_key(&self) -> Result<Option<SecretKey>, StorageUnavailable> {
        Ok(lock(&self.key).clone())
    }

    fn save_device_key(&self, key: &SecretKey) -> Result<(), StorageUnavailable> {
        *lock(&self.key) = Some(key.clone());
        *lock(&self.saved_keys) += 1;
        Ok(())
    }

    fn computer_in_use(&self) -> Option<PublicKey> {
        *lock(&self.in_use)
    }

    fn relay(&self, computer: &PublicKey) -> Option<String> {
        let gate = {
            let mut gate = lock(&self.gate);
            gate.take_if(|gate| gate.computer == *computer)
        };
        if let Some(gate) = gate {
            let _ = gate.entered.send(());
            let _ = gate.release.recv();
        }
        lock(&self.relays).get(computer).cloned()
    }

    fn save_relay(&self, computer: &PublicKey, relay: &str) -> Result<(), StorageUnavailable> {
        lock(&self.relays).insert(*computer, relay.to_string());
        Ok(())
    }

    fn own_relay(&self, computer: &PublicKey) -> Result<Option<String>, StorageUnavailable> {
        Ok(lock(&self.own_relays).get(computer).cloned())
    }

    fn save_own_relay(
        &self,
        computer: &PublicKey,
        line: Option<&str>,
    ) -> Result<(), StorageUnavailable> {
        let mut own_relays = lock(&self.own_relays);
        match line {
            Some(line) => own_relays.insert(*computer, line.to_string()),
            None => own_relays.remove(computer),
        };
        Ok(())
    }
}

impl Host {
    fn device(&self) -> PublicKey {
        let key = SecretKey::generate();
        let public = key.public();
        *lock(&self.key) = Some(key);
        public
    }

    fn use_computer(&self, computer: &Computer) {
        *lock(&self.in_use) = Some(computer.id());
    }

    /// Holds the next dial for `computer`; returns when it has entered the gate,
    /// and the sender that releases it.
    fn hold(&self, computer: &Computer) -> (tokio::sync::oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (entered, entered_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = mpsc::channel();
        *lock(&self.gate) = Some(Gate {
            computer: computer.id(),
            entered,
            release: release_rx,
        });
        (entered_rx, release)
    }
}

#[derive(Clone, Default)]
struct Records(Arc<Mutex<Vec<(&'static str, serde_json::Value)>>>);

impl Records {
    fn logger(&self) -> Logger {
        let records = self.0.clone();
        Arc::new(move |event, fields| lock(&records).push((event, fields)))
    }

    fn of(&self, event: &str) -> Vec<serde_json::Value> {
        lock(&self.0)
            .iter()
            .filter(|(name, _)| *name == event)
            .map(|(_, fields)| fields.clone())
            .collect()
    }

    fn outcomes(&self, event: &str, field: &str) -> Vec<String> {
        self.of(event)
            .iter()
            .map(|fields| fields[field].as_str().unwrap_or_default().to_string())
            .collect()
    }

    async fn wait_for(&self, event: &str) {
        let waited = tokio::time::timeout(WATCHDOG, async {
            while self.of(event).is_empty() {
                // timing: poll — re-reads the records until the event arrives.
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
        waited.await.unwrap_or_else(|_| panic!("no {event} record"));
    }
}

struct Fixture {
    network: Network,
    host: Arc<Host>,
    records: Records,
    node: RemoteNode,
}

fn fixture(timings: Timings) -> Fixture {
    let network = Network::Local(MemoryLookup::new());
    let host = Arc::new(Host::default());
    let records = Records::default();
    let node = RemoteNode::with_timings(network.clone(), host.clone(), records.logger(), timings);
    Fixture {
        network,
        host,
        records,
        node,
    }
}

impl Fixture {
    /// A computer that trusts this device.
    async fn trusting(&self) -> Computer {
        let computer = Computer::start(&self.network, application_alpn()).await;
        computer.trust(self.host.device());
        computer
    }
}

/// Every computer in one fixture trusts the same device key.
async fn trusting_too(fixture: &Fixture, device: PublicKey) -> Computer {
    let computer = Computer::start(&fixture.network, application_alpn()).await;
    computer.trust(device);
    computer
}

async fn answered(node: &RemoteNode) -> bool {
    let (mut send, mut recv) = node.open().await.unwrap();
    send.write_all(b"OPTIONS /probe HTTP/1.1\r\n\r\n")
        .await
        .unwrap();
    send.finish().unwrap();
    recv.read_to_end(4096)
        .await
        .unwrap()
        .starts_with(b"HTTP/1.1 204")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_first_bind_creates_the_key_and_requests_share_one_connection() {
    let f = fixture(Timings::default());
    let computer = Computer::start(&f.network, application_alpn()).await;
    f.host.use_computer(&computer);
    // The device has no key yet, so the first connect makes one and keeps it.
    assert_eq!(f.node.connection().await.unwrap_err(), Failure::NotTrusted);
    assert_eq!(*lock(&f.host.saved_keys), 1);
    computer.trust(lock(&f.host.key).as_ref().unwrap().public());

    let first = f.node.connection().await.unwrap();
    assert!(answered(&f.node).await);
    assert!(same(&first, &f.node.connection().await.unwrap()));
    assert_eq!(*lock(&f.host.saved_keys), 1);
    assert_eq!(f.records.of("remote_endpoint_bind_started").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_removed_device_is_told_without_retrying() {
    let f = fixture(Timings::default());
    let computer = Computer::start(&f.network, application_alpn()).await;
    f.host.device();
    f.host.use_computer(&computer);

    assert_eq!(f.node.connection().await.unwrap_err(), Failure::NotTrusted);
    assert_eq!(
        f.records.outcomes("remote_connect_completed", "outcome"),
        ["not_trusted"]
    );
    assert_eq!(
        f.records.outcomes("remote_connect_completed", "stage"),
        ["refusal_check"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_computer_of_another_protocol_major_is_incompatible() {
    let f = fixture(Timings::default());
    let computer = Computer::start(&f.network, b"openaide/app-server/0".to_vec()).await;
    computer.trust(f.host.device());
    f.host.use_computer(&computer);

    assert_eq!(
        f.node.connection().await.unwrap_err(),
        Failure::Incompatible
    );
    assert_eq!(f.records.of("remote_connect_completed").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unreachable_computer_is_tried_on_a_new_endpoint_each_attempt() {
    let f = fixture(Timings::default());
    f.host.device();
    *lock(&f.host.in_use) = Some(SecretKey::generate().public());

    assert_eq!(f.node.connection().await.unwrap_err(), Failure::Unreachable);
    assert_eq!(f.records.of("remote_connect_completed").len(), 3);
    assert_eq!(f.records.of("remote_endpoint_bind_started").len(), 3);
    assert_eq!(
        f.records.outcomes("remote_endpoint_retired", "reason"),
        ["dial_failed", "dial_failed"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hidden_app_tries_an_unreachable_computer_once() {
    let f = fixture(Timings::default());
    f.host.device();
    *lock(&f.host.in_use) = Some(SecretKey::generate().public());
    f.node.hidden();

    assert_eq!(f.node.connection().await.unwrap_err(), Failure::Unreachable);
    assert_eq!(f.records.of("remote_connect_completed").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_return_to_the_screen_replaces_the_endpoint_and_its_connection() {
    let timings = Timings {
        // timing: data — any absence counts, so the return always starts over.
        stale_after_hidden: Duration::ZERO,
        ..Timings::default()
    };
    let f = fixture(timings);
    let computer = f.trusting().await;
    f.host.use_computer(&computer);
    let before = f.node.connection().await.unwrap();

    f.node.hidden();
    assert!(answered(&f.node).await);
    f.node.shown();
    f.records.wait_for("remote_endpoint_closed").await;

    let after = f.node.connection().await.unwrap();
    assert!(!same(&before, &after));
    assert!(before.close_reason().is_some());
    assert_eq!(f.records.of("remote_endpoint_bind_started").len(), 2);
    assert!(answered(&f.node).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_switch_leaves_the_computer_in_use_until_it_is_adopted() {
    let f = fixture(Timings::default());
    let first = f.trusting().await;
    let device = lock(&f.host.key).as_ref().unwrap().public();
    let second = trusting_too(&f, device).await;
    f.host.use_computer(&first);
    let in_use = f.node.connection().await.unwrap();

    let reached = f.node.reach_for_switch(second.id()).await.unwrap();
    assert!(same(&in_use, &f.node.connection().await.unwrap()));
    assert_eq!(f.node.connection().await.unwrap().remote_id(), first.id());

    f.host.use_computer(&second);
    f.node.adopt(reached.clone());
    assert!(same(&reached, &f.node.connection().await.unwrap()));
    assert!(in_use.close_reason().is_some());
    assert!(answered(&f.node).await);

    // A connection that is in use is not given up as an unused one.
    f.node.abandon(&reached);
    assert!(reached.close_reason().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_return_during_a_switch_dial_waits_for_it_and_keeps_the_endpoint_it_reached() {
    let timings = Timings {
        // timing: data — any absence counts, so the return always marks the endpoint stale.
        stale_after_hidden: Duration::ZERO,
        ..Timings::default()
    };
    let f = fixture(timings);
    let first = f.trusting().await;
    let device = lock(&f.host.key).as_ref().unwrap().public();
    let second = trusting_too(&f, device).await;
    f.host.use_computer(&first);
    f.node.connection().await.unwrap();

    let (entered, release) = f.host.hold(&second);
    let switching = tokio::spawn({
        let node = f.node.clone();
        let target = second.id();
        async move { node.reach_for_switch(target).await }
    });
    entered.await.unwrap();
    f.node.hidden();
    assert!(answered(&f.node).await);
    f.node.shown();
    f.records.wait_for("remote_endpoint_retire_deferred").await;
    release.send(()).unwrap();

    let reached = switching.await.unwrap().unwrap();
    f.records.wait_for("remote_endpoint_kept").await;
    assert!(reached.close_reason().is_none());
    assert!(f.records.of("remote_endpoint_retired").is_empty());
    assert_eq!(f.records.of("remote_endpoint_bind_started").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_switch_dial_is_recorded_and_no_longer_holds_off_a_retire() {
    let timings = Timings {
        // timing: data — any absence counts, so the return always marks the endpoint stale.
        stale_after_hidden: Duration::ZERO,
        ..Timings::default()
    };
    let f = fixture(timings);
    let first = f.trusting().await;
    let device = lock(&f.host.key).as_ref().unwrap().public();
    let second = trusting_too(&f, device).await;
    f.host.use_computer(&first);
    f.node.connection().await.unwrap();

    let (entered, release) = f.host.hold(&second);
    let switching = tokio::spawn({
        let node = f.node.clone();
        let target = second.id();
        async move { node.reach_for_switch(target).await }
    });
    entered.await.unwrap();
    switching.abort();
    release.send(()).unwrap();
    assert!(switching.await.unwrap_err().is_cancelled());
    f.records.wait_for("remote_connect_completed").await;
    assert_eq!(
        f.records.outcomes("remote_connect_completed", "outcome"),
        ["direct", "cancelled"]
    );

    f.node.hidden();
    assert!(answered(&f.node).await);
    f.node.shown();
    // Only an endpoint no switch dial holds is retired.
    f.records.wait_for("remote_endpoint_retired").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_check_names_how_each_computer_answered() {
    let timings = Timings {
        // timing: expiry — the in-memory network has no relay, so the wait for one runs out.
        check_relay_wait: Duration::from_millis(50),
        ..Timings::default()
    };
    let f = fixture(timings);
    let computer = f.trusting().await;
    let stranger = Computer::start(&f.network, application_alpn()).await;
    f.host.use_computer(&computer);

    let fresh = f.node.check(computer.id()).await;
    assert_eq!(fresh.relay, CheckRelay::NotReached);
    assert_eq!(fresh.computer, CheckComputer::Direct);
    assert_eq!(fresh.outcome(), "ok");
    assert_eq!(
        f.node.check(stranger.id()).await.computer,
        CheckComputer::NotTrusted
    );

    f.node.connection().await.unwrap();
    assert_eq!(
        f.node.check(computer.id()).await.computer,
        CheckComputer::Connected
    );
    assert_eq!(
        f.records.outcomes("remote_check_completed", "outcome"),
        ["ok", "relay_not_reached", "ok"]
    );
}

const RELAY_PASSWORD: &str = "right-password";

/// An Own Relay's access control: admits a client that presents [`RELAY_PASSWORD`],
/// and counts whom it admitted and turned away. Clones share the counts.
#[derive(Debug, Default, Clone)]
struct PasswordGate {
    admitted: Arc<AtomicU32>,
    refused: Arc<AtomicU32>,
}

impl AccessControl for PasswordGate {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        if request.auth_token().as_deref() == Some(RELAY_PASSWORD) {
            self.admitted.fetch_add(1, Ordering::SeqCst);
            Access::Allow
        } else {
            self.refused.fetch_add(1, Ordering::SeqCst);
            Access::Deny { reason: None }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_computer_paired_through_its_own_relay_is_dialled_there_with_the_password() {
    let gate = PasswordGate::default();
    let (_, relay_url, _relay_server) =
        iroh::test_utils::run_relay_server_with_access(true, Arc::new(gate.clone()))
            .await
            .unwrap();
    let relay = OwnRelay::parse(&format!("{relay_url}?token={RELAY_PASSWORD}")).unwrap();
    // The device knows no relay, as a phone knows none but the public ones, so it can
    // reach the computer only at the relay its code names.
    let network = Network::Relay(iroh::RelayMap::empty());
    let host = Arc::new(Host::default());
    let records = Records::default();
    let timings = Timings {
        // timing: data — any absence counts, so the return always starts over.
        stale_after_hidden: Duration::ZERO,
        ..Timings::default()
    };
    let node = RemoteNode::with_timings(network.clone(), host.clone(), records.logger(), timings);
    let computer = Computer::start_pairing(&network.with_own_relay(Some(&relay))).await;
    let invite = InviteCode {
        server: computer.id(),
        secret: [5; 16],
        relay: Some(relay.clone()),
    };

    let paired = node.redeem(&invite, "Phone", None).await.unwrap();
    assert_eq!(paired.relay.as_ref(), Some(&relay));
    assert_eq!(
        lock(&host.own_relays).get(&computer.id()),
        Some(&relay.line())
    );

    *lock(&host.in_use) = Some(computer.id());
    let first = node.connection().await.unwrap();
    assert!(answered(&node).await);
    // A return to the screen replaces the endpoint, and the new one learns the relay too.
    node.hidden();
    node.shown();
    records.wait_for("remote_endpoint_closed").await;
    let second = node.connection().await.unwrap();
    assert!(!same(&first, &second));
    assert!(answered(&node).await);

    assert_eq!(
        records.outcomes("remote_connect_completed", "address"),
        ["own_relay", "own_relay"]
    );
    assert!(records
        .of("remote_connect_completed")
        .iter()
        .all(|fields| fields["computer_own_relay"] == true));
    assert_eq!(gate.refused.load(Ordering::SeqCst), 0);
    assert!(
        gate.admitted.load(Ordering::SeqCst) >= 3,
        "the computer and both of the device's endpoints"
    );
    let logged = format!("{:?}", lock(&records.0));
    assert!(!logged.contains(RELAY_PASSWORD));
    assert!(!logged.contains(relay.url().as_str()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_damaged_own_relay_asks_to_pair_again_instead_of_dialling_elsewhere() {
    let f = fixture(Timings::default());
    // Reachable by key on this network, so only the damaged line stops the dial.
    let computer = f.trusting().await;
    f.host.use_computer(&computer);
    lock(&f.host.own_relays).insert(
        computer.id(),
        "http://relay.example.com/?token=abc".to_string(),
    );

    assert_eq!(f.node.connection().await.unwrap_err(), Failure::NotTrusted);
    assert_eq!(
        f.records
            .outcomes("remote_connect_completed", "error_class"),
        ["own_relay_damaged"]
    );
    assert_eq!(
        f.records.outcomes("remote_connect_completed", "stage"),
        ["address"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pairing_again_without_an_own_relay_clears_the_line() {
    let f = fixture(Timings::default());
    f.host.device();
    let computer = Computer::start_pairing(&f.network).await;
    lock(&f.host.own_relays).insert(
        computer.id(),
        "https://relay.example.com/?token=abc".to_string(),
    );
    let invite = InviteCode {
        server: computer.id(),
        secret: [6; 16],
        relay: None,
    };

    f.node.redeem(&invite, "Phone", None).await.unwrap();
    assert!(lock(&f.host.own_relays).is_empty());
    assert_eq!(
        f.records
            .outcomes("remote_pairing_own_relay_kept", "outcome"),
        ["cleared"]
    );

    f.host.use_computer(&computer);
    assert!(answered(&f.node).await);
    assert_eq!(
        f.records.outcomes("remote_connect_completed", "address"),
        ["lookup"]
    );
}
