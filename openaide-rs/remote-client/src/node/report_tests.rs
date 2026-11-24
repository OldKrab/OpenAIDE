use super::*;

fn server() -> String {
    format!("ab12cd34ef56{}", "0".repeat(52))
}

const WIFI: NetworkKind = NetworkKind {
    transport: Transport::Wifi,
    validated: true,
    vpn: false,
};

#[test]
fn a_dial_that_timed_out_says_how_far_it_got_and_what_it_knew() {
    let mut report = DialReport::new(Purpose::Switch, 1, &server(), WIFI);
    report.stage = Stage::Connect;
    report.address = "lookup";
    report.own_relay = Some(true);
    report.known_relay = Some(false);
    report.known_direct = Some(0);

    assert_eq!(
        report.fields(3, "timeout", 4_000, None),
        json!({
            "attempt": 3, "purpose": "switch", "target": "ab12cd34", "index": 1,
            "outcome": "timeout", "duration_ms": 4_000,
            "network": "wifi", "network_validated": true, "vpn": false,
            "stage": "connect", "address": "lookup",
            "own_relay": "present", "known_relay": "absent", "known_direct": 0,
        })
    );
}

#[test]
fn a_failure_before_the_endpoint_is_named_by_its_class_at_the_bind_stage() {
    let report = DialReport::new(Purpose::Live, 0, &server(), NetworkKind::NONE);
    let fields = report.fields(1, "failed", 12, Some("no_address"));

    assert_eq!(fields["error_class"], "no_address");
    assert_eq!(fields["stage"], "bind");
    assert_eq!(fields["network"], "none");
    assert!(fields.get("own_relay").is_none());
}

#[test]
fn the_target_is_a_short_prefix_of_the_public_key() {
    assert_eq!(short_id(&server()), "ab12cd34");
    assert_eq!(short_id("--"), "none");
}

#[test]
fn a_vpn_is_reported_beside_the_transport_under_it() {
    assert_eq!(
        NetworkKind::of(false, true, false, true, true),
        NetworkKind {
            transport: Transport::Cellular,
            validated: true,
            vpn: true
        }
    );
    assert_eq!(
        NetworkKind::of(false, false, false, true, false).transport,
        Transport::Vpn
    );
}
