//! Proves the edge can be reached through its home relay, and says when to rebind it.
//!
//! A device that knows the App Server only by its relay connects through that relay.
//! iroh can keep a relay connection it still reports as home after it has gone silent:
//! no pings in either direction for hours, while the endpoint answers direct dials. iroh
//! never redials it, so every device off the local network sees an unreachable computer
//! until the endpoint is bound again. Nothing the endpoint reports tells this apart from
//! a healthy relay; only a dial through it does.
//!
//! The check dials the edge from a throwaway relay-only endpoint, addressed by the edge's
//! home relay alone. The edge answers the check's protocol by closing the connection, so
//! a completed handshake means the relay carries traffic both ways.

use std::time::{Duration, Instant};

use iroh::endpoint::{presets, Connection};
use iroh::tls::CaTlsConfig;
use iroh::{Endpoint, EndpointAddr, RelayMode, Watcher};

use crate::logging;

/// The edge offers this protocol only to answer a check; it serves nothing on it.
pub(crate) const RELAY_CHECK_ALPN: &[u8] = b"openaide/relay-check/1";
/// How often the edge is checked while it is bound.
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_secs(300);
/// Consecutive unreached checks before the edge is bound again. One alone can be a
/// relay restarting or a passing network fault that iroh recovers from by itself.
const FAILURES_BEFORE_REBIND: u32 = 2;
const ONLINE_TIMEOUT: Duration = Duration::from_secs(10);
const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
/// Closing an endpoint whose relay is wedged may wait on that relay.
pub(crate) const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSE_CHECKED: u32 = 0;

/// What one check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckOutcome {
    /// A dial through the edge's home relay completed its handshake.
    Reached,
    /// The check's own endpoint could not use a relay, so it says nothing about the edge.
    Skipped(&'static str),
    /// The edge could not be reached through its relay, at the named stage.
    Unreached(&'static str),
}

impl CheckOutcome {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Reached => "reached",
            Self::Skipped(_) => "skipped",
            Self::Unreached(_) => "unreached",
        }
    }

    pub(crate) fn stage(self) -> Option<&'static str> {
        match self {
            Self::Reached => None,
            Self::Skipped(stage) | Self::Unreached(stage) => Some(stage),
        }
    }
}

/// Counts consecutive unreached checks. Owned by the edge's loop.
#[derive(Debug, Default)]
pub(crate) struct RelayHealth {
    failures: u32,
}

impl RelayHealth {
    /// Records one check and returns whether the edge should be bound again now.
    /// A skipped check neither counts against the edge nor clears earlier failures.
    pub(crate) fn record(&mut self, outcome: CheckOutcome) -> bool {
        match outcome {
            CheckOutcome::Reached => {
                self.failures = 0;
                false
            }
            CheckOutcome::Skipped(_) => false,
            CheckOutcome::Unreached(_) => {
                self.failures += 1;
                if self.failures >= FAILURES_BEFORE_REBIND {
                    self.failures = 0;
                    return true;
                }
                false
            }
        }
    }

    pub(crate) fn failures(&self) -> u32 {
        self.failures
    }
}

/// The relay settings the check dials with: the edge's own.
#[derive(Clone)]
pub(crate) struct RelayCheck {
    relay_mode: RelayMode,
    ca_tls: Option<CaTlsConfig>,
}

impl RelayCheck {
    pub(crate) fn new(relay_mode: RelayMode, ca_tls: Option<CaTlsConfig>) -> Self {
        Self { relay_mode, ca_tls }
    }

    /// Dials the edge at [`edge`], its current address, through its home relay alone,
    /// from a new endpoint that has no direct path.
    pub(crate) async fn check(&self, edge: &EndpointAddr) -> CheckOutcome {
        let Some(relay) = edge.relay_urls().next().cloned() else {
            return CheckOutcome::Unreached("no_home_relay");
        };
        let mut builder = Endpoint::builder(presets::Minimal)
            .relay_mode(self.relay_mode.clone())
            .clear_ip_transports();
        if let Some(ca_tls) = &self.ca_tls {
            builder = builder.ca_tls_config(ca_tls.clone());
        }
        let Ok(probe) = builder.bind().await else {
            return CheckOutcome::Skipped("bind");
        };
        let outcome = if tokio::time::timeout(ONLINE_TIMEOUT, probe.online())
            .await
            .is_err()
        {
            CheckOutcome::Skipped("offline")
        } else {
            let address = EndpointAddr::new(edge.id).with_relay_url(relay);
            match tokio::time::timeout(DIAL_TIMEOUT, probe.connect(address, RELAY_CHECK_ALPN)).await
            {
                Ok(Ok(connection)) => {
                    connection.close(CLOSE_CHECKED.into(), b"checked");
                    CheckOutcome::Reached
                }
                Ok(Err(_error)) => CheckOutcome::Unreached("connect"),
                Err(_elapsed) => CheckOutcome::Unreached("timeout"),
            }
        };
        let _ = tokio::time::timeout(CLOSE_TIMEOUT, probe.close()).await;
        outcome
    }
}

/// Answers a check's connection on the edge: the handshake was the answer.
pub(crate) fn answer(connection: Connection) {
    connection.close(CLOSE_CHECKED.into(), b"checked");
}

/// Logs each time [`endpoint`] gains or loses its home relay, until it closes. Without
/// a relay, a device off the local network cannot reach the App Server.
pub(crate) fn watch_home_relay(endpoint: &Endpoint) {
    let mut statuses = endpoint.home_relay_status();
    let closed = endpoint.closed();
    tokio::spawn(async move {
        let watch = async {
            let mut connected = None;
            let mut changed_at = Instant::now();
            loop {
                let now = statuses.get().iter().any(|status| status.is_connected());
                if connected != Some(now) {
                    let fields = serde_json::json!({
                        "connected": now,
                        "previous_duration_ms": connected.map(|_| elapsed_ms(changed_at)),
                    });
                    if now {
                        logging::info("remote_devices_relay_changed", fields);
                    } else {
                        logging::warn("remote_devices_relay_changed", fields);
                    }
                    connected = Some(now);
                    changed_at = Instant::now();
                }
                if statuses.updated().await.is_err() {
                    break;
                }
            }
        };
        tokio::select! {
            _ = watch => {}
            _ = closed => {}
        }
    });
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "relay_check_tests.rs"]
mod tests;
