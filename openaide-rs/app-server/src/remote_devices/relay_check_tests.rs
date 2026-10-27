use iroh::test_utils::run_relay_server;
use iroh::SecretKey;

use super::*;
use crate::remote_devices::edge::{bind, EdgeNetwork};

#[test]
fn rebinds_after_consecutive_unreached_checks_only() {
    let mut health = RelayHealth::default();
    assert!(!health.record(CheckOutcome::Unreached("timeout")));
    assert!(!health.record(CheckOutcome::Reached));
    assert!(!health.record(CheckOutcome::Unreached("timeout")));
    assert!(health.record(CheckOutcome::Unreached("connect")));
    assert_eq!(health.failures(), 0);
}

#[test]
fn a_skipped_check_neither_counts_nor_clears_failures() {
    let mut health = RelayHealth::default();
    assert!(!health.record(CheckOutcome::Unreached("timeout")));
    assert!(!health.record(CheckOutcome::Skipped("offline")));
    assert_eq!(health.failures(), 1);
    assert!(health.record(CheckOutcome::Unreached("timeout")));
}

/// The edge as the App Server binds it, answering checks the way its accept loop does.
async fn edge_on(network: &EdgeNetwork) -> Endpoint {
    let edge = bind(SecretKey::generate(), network).await.unwrap();
    edge.online().await;
    let accepting = edge.clone();
    tokio::spawn(async move {
        while let Some(incoming) = accepting.accept().await {
            if let Ok(connection) = incoming.await {
                assert_eq!(connection.alpn(), RELAY_CHECK_ALPN);
                answer(connection);
            }
        }
    });
    edge
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_reaches_an_edge_through_its_relay() {
    let (map, _url, _relay) = run_relay_server().await.unwrap();
    let network = EdgeNetwork::Relay(map);
    let edge = edge_on(&network).await;

    let outcome = network_check(&network).check(&edge.addr()).await;

    assert_eq!(outcome, CheckOutcome::Reached);
}

/// The failure seen on a Driver: the relay works and the edge still names it home, but
/// nothing reaches the edge through it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_does_not_reach_an_edge_the_relay_no_longer_carries() {
    let (map, _url, _relay) = run_relay_server().await.unwrap();
    let network = EdgeNetwork::Relay(map);
    let edge = edge_on(&network).await;
    let address = edge.addr();
    edge.close().await;

    let outcome = network_check(&network).check(&address).await;

    assert!(
        matches!(outcome, CheckOutcome::Unreached("timeout" | "connect")),
        "{outcome:?}"
    );
}

fn network_check(network: &EdgeNetwork) -> RelayCheck {
    network.relay_check().expect("a relay network is checked")
}
