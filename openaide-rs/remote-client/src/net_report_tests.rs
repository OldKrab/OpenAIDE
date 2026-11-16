use iroh::unstable_net_report::NetReport;

use super::NetSummary;

#[test]
fn the_summary_keeps_what_the_network_allows_and_drops_its_addresses() {
    let mut report = NetReport::default();
    report.udp_v4 = true;
    report.mapping_varies_by_dest_ipv4 = Some(true);
    report.global_v4 = Some("203.0.113.7:4433".parse().unwrap());

    let summary = serde_json::to_value(NetSummary::of(&report, false)).unwrap();

    assert_eq!(
        summary,
        serde_json::json!({
            "udp_v4": true,
            "udp_v6": false,
            "varies_by_dest_v4": true,
            "varies_by_dest_v6": null,
            "public_v4": true,
            "public_v6": false,
            "captive_portal": null,
            "portmapped": false,
        })
    );
    assert!(!summary.to_string().contains("203.0.113.7"));
}
