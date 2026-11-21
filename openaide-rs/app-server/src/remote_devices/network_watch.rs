//! Explains why a Remote Device stays on a relay instead of a direct path.
//!
//! Two views, both metadata-only: what the App Server's network allows (logged when it
//! changes), and how long each device connection actually spent direct.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iroh::endpoint::Connection;
use iroh::unstable_net_report::NetReport;
use iroh::{Endpoint, Watcher};
use openaide_app_server_protocol::devices::{RemoteDeviceConnection, RemoteDevicePath};
use serde::Serialize;

use super::RemoteDevices;
use crate::logging;

/// Logs the App Server's network summary each time it changes, until `endpoint` closes.
// TODO: duplicates `openaide_remote_client::watch_net_report`, like `watch_rate_limits`
// in relay_check.rs. Share both once the App Server can depend on a crate common to it and
// the client.
pub(crate) fn watch_net_report(endpoint: &Endpoint) {
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
                    logging::info(
                        "remote_devices_network_changed",
                        serde_json::to_value(&summary).unwrap_or_default(),
                    );
                    last = Some(summary);
                }
            }
            if reports.updated().await.is_err() {
                break;
            }
        }
    });
}

/// What the network allows for a direct path, without any address.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct NetSummary {
    udp_v4: bool,
    udp_v6: bool,
    /// The NAT gives a different public port per destination, which defeats hole punching
    /// unless the device's side is reachable.
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

/// How one connection's time split between a direct path and a relay. The path is
/// sampled, so the split is accurate to the sampling interval.
#[derive(Debug)]
pub(crate) struct RouteTally {
    started: Instant,
    direct_since: Option<Instant>,
    direct: Duration,
    first_direct: Option<Duration>,
}

impl RouteTally {
    pub(crate) fn new(started: Instant) -> Self {
        Self {
            started,
            direct_since: None,
            direct: Duration::ZERO,
            first_direct: None,
        }
    }

    /// Records the route seen at `now`.
    pub(crate) fn observe(&mut self, direct: bool, now: Instant) {
        match (direct, self.direct_since) {
            (true, None) => {
                self.direct_since = Some(now);
                self.first_direct.get_or_insert(now - self.started);
            }
            (false, Some(since)) => {
                self.direct += now - since;
                self.direct_since = None;
            }
            _ => {}
        }
    }

    /// The fields for the connection's end at `now`: time spent direct, and how long the
    /// first direct path took to form, or null when none ever did.
    pub(crate) fn fields(&self, now: Instant) -> serde_json::Value {
        let open = self
            .direct_since
            .map_or(Duration::ZERO, |since| now - since);
        serde_json::json!({
            "direct_ms": (self.direct + open).as_millis() as u64,
            "first_direct_ms": self.first_direct.map(|took| took.as_millis() as u64),
        })
    }
}

/// How often a device connection's path is sampled.
const PATH_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Reports a move between a direct path and a relay. The loop is silent unless the path changes.
pub(crate) async fn watch_path(
    connection: Connection,
    devices: RemoteDevices,
    tally: Arc<Mutex<RouteTally>>,
) {
    let device = connection.remote_id();
    let mut interval = tokio::time::interval(PATH_POLL_INTERVAL);
    interval.tick().await;
    let mut route = path_name(&describe(&connection));
    loop {
        interval.tick().await;
        let current = describe(&connection);
        tally
            .lock()
            .expect("route tally lock poisoned")
            .observe(is_direct(&current), Instant::now());
        // A route change can explain a stall on the device's side.
        if path_name(&current) != route {
            route = path_name(&current);
            logging::info(
                "remote_device_route_changed",
                serde_json::json!({ "device": device.fmt_short().to_string(), "route": route }),
            );
        }
        devices.device_path_changed(&device, current);
    }
}

pub(crate) fn describe(connection: &Connection) -> RemoteDeviceConnection {
    let paths = connection.paths();
    let selected = paths.iter().find(|path| path.is_selected());
    match selected.map(|path| path.remote_addr().clone()) {
        Some(iroh::TransportAddr::Ip(address)) => RemoteDeviceConnection {
            path: RemoteDevicePath::Direct,
            address: Some(address.ip().to_string()),
        },
        _ => RemoteDeviceConnection {
            path: RemoteDevicePath::Relayed,
            address: None,
        },
    }
}

pub(crate) fn is_direct(connection: &RemoteDeviceConnection) -> bool {
    matches!(connection.path, RemoteDevicePath::Direct)
}

pub(crate) fn path_name(connection: &RemoteDeviceConnection) -> &'static str {
    match connection.path {
        RemoteDevicePath::Direct => "direct",
        RemoteDevicePath::Relayed => "relayed",
    }
}

#[cfg(test)]
#[path = "network_watch_tests.rs"]
mod tests;
