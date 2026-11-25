//! Reaching a computer: the attempts of one connect, each on the current
//! endpoint and each ending in one `remote_connect_completed` record.

use std::time::{Duration, Instant};

use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointAddr, PublicKey, RelayUrl, TransportAddr};
use openaide_remote_core::OwnRelay;

use super::policy;
use super::report::{DialReport, Purpose, Stage};
use super::RemoteNode;
use crate::client::{
    connect_stage, elapsed_ms, failure_of, names_no_protocol, server_address, Failure,
};

/// Why one attempt failed: the failure for the caller, the record's outcome
/// word, and the error's variant when the transport named one.
struct Missed {
    failure: Failure,
    outcome: &'static str,
    error_class: Option<&'static str>,
}

impl Missed {
    fn of(failure: Failure) -> Self {
        Self {
            failure,
            outcome: failure.class(),
            error_class: None,
        }
    }
}

/// One attempt under way. Dropped unfinished, it was cancelled: a cancelled switch,
/// not an unreachable computer. It still writes its record and releases the
/// endpoint it held off a retire.
struct Attempt {
    node: RemoteNode,
    number: u32,
    started: Instant,
    report: DialReport,
    endpoint: Option<Endpoint>,
    /// Counted as a switch's dial on the endpoint, so not yet released.
    counted: bool,
    finished: bool,
}

impl Attempt {
    fn finish(&mut self, outcome: &str, error_class: Option<&str>) {
        self.finished = true;
        let fields =
            self.report
                .fields(self.number, outcome, elapsed_ms(self.started), error_class);
        self.node.log("remote_connect_completed", fields);
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if self.counted {
            let node = self.node.clone();
            self.node
                .inner
                .runtime
                .spawn(async move { node.end_switch_dial(false).await });
        }
        self.finish("cancelled", None);
    }
}

impl RemoteNode {
    /// Every attempt of one connect to `server`, within the connect budget.
    pub(super) async fn reach(
        &self,
        server: PublicKey,
        purpose: Purpose,
    ) -> Result<Connection, Failure> {
        let timings = self.inner.timings;
        let started = Instant::now();
        let mut index = 0;
        loop {
            let timeout = timings
                .attempt_timeout(index, started.elapsed())
                .ok_or(Failure::Unreachable)?;
            let failure = match self.dial(server, index, timeout, purpose).await {
                Ok(connection) => return Ok(connection),
                Err(failure) => failure,
            };
            index += 1;
            // A hidden app usually has no network to retry on; its return starts over anyway.
            let more =
                !self.is_hidden() && timings.attempt_timeout(index, started.elapsed()).is_some();
            if failure != Failure::Unreachable || !more {
                return Err(failure);
            }
            if policy::retire_after_failed_dial(self.live().is_some()) {
                self.mark_stale("dial_failed");
            }
        }
    }

    /// One attempt to reach `server` on the current endpoint.
    pub(super) async fn dial(
        &self,
        server: PublicKey,
        index: usize,
        timeout: Duration,
        purpose: Purpose,
    ) -> Result<Connection, Failure> {
        let number = self
            .inner
            .attempts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let report = DialReport::new(
            purpose,
            index,
            &server.to_string(),
            self.inner.host.network(),
        );
        self.log("remote_connect_started", report.start_fields(number));
        let mut attempt = Attempt {
            node: self.clone(),
            number,
            started: Instant::now(),
            report,
            endpoint: None,
            counted: false,
            finished: false,
        };
        let result = self
            .attempt(server, index, timeout, purpose, &mut attempt)
            .await;
        let (outcome, error_class) = match &result {
            Ok(connection) => (route(connection), None),
            Err(missed) => (missed.outcome, missed.error_class),
        };
        // What the endpoint learned of the computer tells a failed lookup from an unanswered dial.
        if matches!(outcome, "timeout" | "failed") {
            if let Some(endpoint) = attempt.endpoint.clone() {
                self.known_address(&endpoint, server, &mut attempt.report)
                    .await;
            }
        }
        if std::mem::take(&mut attempt.counted) {
            let node = self.clone();
            let succeeded = result.is_ok();
            // Spawned, so a cancel from here on cannot leave the dial counted.
            let _ = self
                .inner
                .runtime
                .spawn(async move { node.end_switch_dial(succeeded).await })
                .await;
        }
        attempt.finish(outcome, error_class);
        result.map_err(|missed| missed.failure)
    }

    async fn attempt(
        &self,
        server: PublicKey,
        index: usize,
        timeout: Duration,
        purpose: Purpose,
        attempt: &mut Attempt,
    ) -> Result<Connection, Missed> {
        // The live connection and a switch share the one endpoint (see the node); a
        // switch's dial holds off a retire the live connection's reconnect would make.
        let endpoint = if purpose == Purpose::Switch {
            let endpoint = self.switch_dial_endpoint().await.map_err(Missed::of)?;
            attempt.counted = true;
            endpoint
        } else {
            self.endpoint().await.map_err(Missed::of)?
        };
        attempt.endpoint = Some(endpoint.clone());
        let report = &mut attempt.report;
        report.stage = Stage::Address;
        report.own_relay = Some(endpoint.addr().relay_urls().next().is_some());
        let own = self.computer_own_relay(server)?;
        report.computer_own_relay = Some(own.is_some());
        let (address, relay) = match own {
            // Every attempt runs on whichever endpoint is current, possibly a new one,
            // so each teaches its endpoint the relay and its password again (ADR-0085).
            // There is no fallback: the computer is not on the public relays.
            Some(own) => {
                report.address = "own_relay";
                report.stage = Stage::Connect;
                let address = server_address(&endpoint, server, Some(&own)).await;
                (address, Some(own.url().clone()))
            }
            None => {
                let remembered = self.inner.host.relay(&server);
                let relay = policy::relay_for(index, remembered.as_deref())
                    .and_then(|relay| relay.parse::<RelayUrl>().ok());
                report.address = if relay.is_some() {
                    "remembered_relay"
                } else {
                    "lookup"
                };
                report.stage = Stage::Connect;
                let address = match relay.clone() {
                    Some(relay) => EndpointAddr::new(server).with_relay_url(relay),
                    None => EndpointAddr::new(server),
                };
                (address, relay)
            }
        };
        let connecting = endpoint.connect(address, &self.inner.alpn);
        let connection = match tokio::time::timeout(timeout, connecting).await {
            Ok(Ok(connection)) => connection,
            // An App Server of another protocol major does not offer this protocol.
            Ok(Err(error)) if names_no_protocol(&error.to_string()) => {
                return Err(Missed {
                    failure: Failure::Incompatible,
                    outcome: Failure::Incompatible.class(),
                    error_class: Some("protocol"),
                })
            }
            Ok(Err(error)) => {
                return Err(Missed {
                    failure: Failure::Unreachable,
                    outcome: "failed",
                    error_class: Some(connect_stage(&error)),
                })
            }
            Err(_) => {
                return Err(Missed {
                    failure: Failure::Unreachable,
                    outcome: "timeout",
                    error_class: None,
                })
            }
        };
        // The App Server completes the handshake before it checks the key, so a
        // removed device learns it from the close that follows.
        report.stage = Stage::RefusalCheck;
        let refusal = tokio::time::timeout(self.inner.timings.refusal_wait, connection.closed());
        if let Ok(reason) = refusal.await {
            return Err(Missed::of(failure_of(&reason)));
        }
        report.stage = Stage::Done;
        self.remember_relay(&connection, server, relay.as_ref(), attempt.number);
        Ok(connection)
    }

    /// The Own Relay `server` was paired through, read for every attempt. A line that
    /// no longer reads is a damaged pairing: without it the computer cannot be found,
    /// so the user is asked to pair again rather than dialled on the public relays.
    fn computer_own_relay(&self, server: PublicKey) -> Result<Option<OwnRelay>, Missed> {
        let line = self.inner.host.own_relay(&server).map_err(|_| Missed {
            failure: Failure::Storage,
            outcome: Failure::Storage.class(),
            error_class: Some("own_relay_storage"),
        })?;
        let Some(line) = line else {
            return Ok(None);
        };
        OwnRelay::parse(&line).map(Some).map_err(|_| Missed {
            failure: Failure::NotTrusted,
            outcome: Failure::NotTrusted.class(),
            error_class: Some("own_relay_damaged"),
        })
    }

    async fn known_address(&self, endpoint: &Endpoint, server: PublicKey, report: &mut DialReport) {
        let wait = self.inner.timings.known_address_wait;
        let Ok(Some(info)) = tokio::time::timeout(wait, endpoint.remote_info(server)).await else {
            return;
        };
        report.known_relay = Some(info.addrs().any(|address| address.addr().is_relay()));
        report.known_direct = Some(
            info.addrs()
                .filter(|address| address.addr().is_ip())
                .count(),
        );
    }

    /// Keeps the relay the computer answered through, so the next connect can skip
    /// the address lookup. For a computer on an Own Relay, `used` is that relay, so
    /// the hint normally stays as it was; it is never read for such a computer.
    fn remember_relay(
        &self,
        connection: &Connection,
        server: PublicKey,
        used: Option<&RelayUrl>,
        attempt: u32,
    ) {
        let paths = connection.paths();
        let current = paths.iter().find_map(|path| match path.remote_addr() {
            TransportAddr::Relay(url) => Some(url.to_string()),
            _ => None,
        });
        let outcome = match current {
            Some(current) if policy::valid_relay(&current) => {
                if used.is_some_and(|used| used.to_string() == current) {
                    "unchanged"
                } else if self.inner.host.save_relay(&server, &current).is_ok() {
                    "saved"
                } else {
                    "storage_unavailable"
                }
            }
            _ => "unavailable",
        };
        self.log(
            "remote_connect_address",
            serde_json::json!({ "attempt": attempt, "outcome": outcome }),
        );
    }
}

/// How the connection reaches the computer now: through a relay or directly.
pub(super) fn route(connection: &Connection) -> &'static str {
    let paths = connection.paths();
    let relayed = paths
        .iter()
        .any(|path| path.is_selected() && path.is_relay());
    if relayed {
        "relay"
    } else {
        "direct"
    }
}
