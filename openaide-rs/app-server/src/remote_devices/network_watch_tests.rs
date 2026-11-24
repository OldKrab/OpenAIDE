// timing-file: data — the route tally takes explicit instants; nothing waits.
use std::time::{Duration, Instant};

use iroh::unstable_net_report::NetReport;

use super::*;

#[test]
fn the_summary_keeps_what_the_network_allows_and_drops_its_addresses() {
    let mut report = NetReport::default();
    report.udp_v6 = true;
    report.global_v6 = Some("[2001:db8::7]:4433".parse().unwrap());

    let summary = serde_json::to_value(NetSummary::of(&report, true)).unwrap();

    assert_eq!(summary["udp_v6"], true);
    assert_eq!(summary["public_v6"], true);
    assert_eq!(summary["portmapped"], true);
    assert!(!summary.to_string().contains("2001:db8"));
}

#[test]
fn a_connection_that_went_direct_reports_when_and_for_how_long() {
    let start = Instant::now();
    let at = |secs| start + Duration::from_secs(secs);
    let mut tally = RouteTally::new(start);

    tally.observe(false, at(5));
    tally.observe(true, at(10));
    tally.observe(false, at(30));
    tally.observe(true, at(40));

    assert_eq!(
        tally.fields(at(45)),
        serde_json::json!({
            "direct_ms": 25_000,
            "first_direct_ms": 10_000,
            "relayed_tx_bytes": 0,
            "relayed_rx_bytes": 0,
        })
    );
}

#[test]
fn a_connection_that_never_went_direct_says_so() {
    let start = Instant::now();
    let mut tally = RouteTally::new(start);

    tally.observe(false, start + Duration::from_secs(5));

    assert_eq!(
        tally.fields(start + Duration::from_secs(9)),
        serde_json::json!({
            "direct_ms": 0,
            "first_direct_ms": null,
            "relayed_tx_bytes": 0,
            "relayed_rx_bytes": 0,
        })
    );
}

#[test]
fn relayed_bytes_add_up_across_relay_paths_and_keep_each_paths_last_sample() {
    let start = Instant::now();
    let mut tally = RouteTally::new(start);

    tally.observe_relay_path(PathId::from(0u32), 1_000, 200);
    // The same path sampled again: its counters grew, and the new sample replaces the old.
    tally.observe_relay_path(PathId::from(0u32), 5_000, 900);
    // A second relay path, opened after the first one closed.
    tally.observe_relay_path(PathId::from(3u32), 40, 60);

    let fields = tally.fields(start);
    assert_eq!(fields["relayed_tx_bytes"], 5_040);
    assert_eq!(fields["relayed_rx_bytes"], 960);
}
