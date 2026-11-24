//! Reports what this endpoint's network allows for a direct connection.
//!
//! A Remote Device falls back to a relay when no direct path forms. Whether one can form
//! depends on the networks at both ends: UDP reachability, a NAT that changes the public
//! port for each destination (which defeats hole punching), a public IPv6 address, and a
//! router that accepts a port mapping. iroh measures these in its network report; this
//! module logs a metadata-only summary whenever it changes. Addresses stay out of logs.

use iroh::unstable_net_report::NetReport;
use iroh::{Endpoint, Watcher};
use serde::Serialize;

use crate::Logger;

/// Logs `event` with the network summary each time it changes, until `endpoint` closes.
pub fn watch_net_report(endpoint: &Endpoint, log: Logger, event: &'static str) {
    let endpoint = endpoint.clone();
    let mut reports = endpoint.net_report();
    tokio::spawn(async move {
        let mut last = None;
        loop {
            if let Some(report) = reports.get() {
                let portmapped = endpoint
                    .metrics()
                    .net_report
                    .portmap_external_address_updated
                    .get()
                    > 0;
                let summary = NetSummary::of(&report, portmapped);
                if last.as_ref() != Some(&summary) {
                    (log)(event, serde_json::to_value(&summary).unwrap_or_default());
                    last = Some(summary);
                }
            }
            // Ends with the endpoint.
            if reports.updated().await.is_err() {
                break;
            }
        }
    });
}

/// What the network allows, without any address.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct NetSummary {
    udp_v4: bool,
    udp_v6: bool,
    /// The NAT gives a different public port per destination; hole punching then fails
    /// unless the other side is reachable.
    varies_by_dest_v4: Option<bool>,
    varies_by_dest_v6: Option<bool>,
    public_v4: bool,
    public_v6: bool,
    captive_portal: Option<bool>,
    /// The router has granted a port mapping at some point since the endpoint started.
    portmapped: bool,
}

impl NetSummary {
    pub(crate) fn of(report: &NetReport, portmapped: bool) -> Self {
        Self {
            udp_v4: report.udp_v4,
            udp_v6: report.udp_v6,
            varies_by_dest_v4: report.mapping_varies_by_dest_ipv4,
            varies_by_dest_v6: report.mapping_varies_by_dest_ipv6,
            public_v4: report.global_v4.is_some(),
            public_v6: report.global_v6.is_some(),
            captive_portal: report.captive_portal,
            portmapped,
        }
    }
}

#[cfg(test)]
#[path = "net_report_tests.rs"]
mod tests;
