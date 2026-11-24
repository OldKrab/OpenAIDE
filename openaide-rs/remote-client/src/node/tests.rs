//! The node against App Servers on the in-memory network: each fake computer
//! answers every request of a trusted device and closes a connection from any
//! other key as the App Server does.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::Duration;

use iroh::address_lookup::MemoryLookup;
use openaide_remote_core::close;

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
        let endpoint = network
            .builder()
            .secret_key(SecretKey::generate())
            .alpns(vec![alpn])
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
