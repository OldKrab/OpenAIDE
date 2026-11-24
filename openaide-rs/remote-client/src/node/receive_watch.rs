//! Notices when the connection in use stops receiving from the computer while
//! the app is on screen.
//!
//! The transport keeps a connection alive with a heartbeat every few seconds, so a
//! healthy connection receives something well within the stall limit even when the
//! workspace is idle. A connection that only sends is the failure this reports: the
//! user sees "Reconnecting…" while the connection still looks open. Healthy
//! connections stay quiet; each stall yields one report and one end.

use std::time::{Duration, Instant};

use iroh::endpoint::{Connection, ConnectionStats};

use super::RemoteNode;

/// A change worth one record; each carries how long the silence had lasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    /// Nothing has arrived for this long while on screen.
    Stalled(Duration),
    /// Data arrived again after this much silence.
    Resumed(Duration),
    /// The app left the screen during a stall that had lasted this long.
    Hidden(Duration),
    /// The connection ended or was replaced during a stall that had lasted this long.
    Ended(Duration),
}

impl Change {
    fn outcome(self) -> &'static str {
        match self {
            Self::Stalled(_) => "stalled",
            Self::Resumed(_) => "resumed",
            Self::Hidden(_) => "hidden",
            Self::Ended(_) => "ended",
        }
    }

    fn silent(self) -> Duration {
        match self {
            Self::Stalled(silent)
            | Self::Resumed(silent)
            | Self::Hidden(silent)
            | Self::Ended(silent) => silent,
        }
    }
}

/// The rules of the watch, fed the received-datagram count; kept free of the
/// network so they can be tested. Times are offsets from the watch's start.
#[derive(Debug)]
pub(crate) struct ReceiveWatch {
    stall_after: Duration,
    /// When something was last received, or when watching started over; `None` while hidden.
    quiet_since: Option<Duration>,
    last_rx: Option<u64>,
    reported: bool,
}

impl ReceiveWatch {
    pub fn new(stall_after: Duration) -> Self {
        Self {
            stall_after,
            quiet_since: None,
            last_rx: None,
            reported: false,
        }
    }

    /// One sample of the received-datagram count `rx` at `now`.
    pub fn sample(&mut self, now: Duration, rx: u64, visible: bool) -> Option<Change> {
        let since = self.quiet_since;
        if !visible {
            self.quiet_since = None;
            self.last_rx = Some(rx);
            let since = since.filter(|_| self.reported)?;
            self.reported = false;
            return Some(Change::Hidden(now.saturating_sub(since)));
        }
        if since.is_none() || self.last_rx != Some(rx) {
            self.last_rx = Some(rx);
            self.quiet_since = Some(now);
            let since = since.filter(|_| self.reported)?;
            self.reported = false;
            return Some(Change::Resumed(now.saturating_sub(since)));
        }
        let silent = now.saturating_sub(since?);
        if !self.reported && silent >= self.stall_after {
            self.reported = true;
            return Some(Change::Stalled(silent));
        }
        None
    }

    /// The connection is no longer watched.
    pub fn end(&mut self, now: Duration) -> Option<Change> {
        let since = self.quiet_since.filter(|_| self.reported)?;
        self.reported = false;
        Some(Change::Ended(now.saturating_sub(since)))
    }
}

impl RemoteNode {
    /// Records when `opened` stops receiving while the app is on screen, for as
    /// long as it stays the connection in use. The end record says whether this
    /// device kept sending and losing packets meanwhile, which tells a dead return
    /// path from a computer that stopped answering.
    // TODO: once a stall's cause is known from these records, decide whether a
    // stall should replace the endpoint as a return to the screen does.
    pub(super) fn watch_receive(&self, opened: Connection, attempt: u32) {
        let node = self.clone();
        self.inner.runtime.spawn(async move {
            let timings = node.inner.timings;
            let started = Instant::now();
            let mut watch = ReceiveWatch::new(timings.stall_after);
            let mut at_stall: Option<ConnectionStats> = None;
            while node.is_in_use(&opened) && opened.close_reason().is_none() {
                let stats = opened.stats();
                let change =
                    watch.sample(started.elapsed(), stats.udp_rx.datagrams, !node.is_hidden());
                if let Some(change) = change {
                    if matches!(change, Change::Stalled(_)) {
                        at_stall = Some(stats.clone());
                    }
                    node.record_receive(change, attempt, &opened, &stats, at_stall.as_ref());
                }
                tokio::time::sleep(timings.receive_sample).await;
            }
            if let Some(ended) = watch.end(started.elapsed()) {
                node.record_receive(ended, attempt, &opened, &opened.stats(), at_stall.as_ref());
            }
        });
    }

    fn record_receive(
        &self,
        change: Change,
        attempt: u32,
        opened: &Connection,
        stats: &ConnectionStats,
        at_stall: Option<&ConnectionStats>,
    ) {
        let mut fields = serde_json::json!({
            "outcome": change.outcome(),
            "attempt": attempt,
            "silent_ms": millis(change.silent()),
            "route": "none",
            "rx_datagrams": stats.udp_rx.datagrams,
            "tx_datagrams": stats.udp_tx.datagrams,
            "lost_packets": stats.lost_packets,
        });
        let paths = opened.paths();
        if let Some(selected) = paths.iter().find(|path| path.is_selected()) {
            fields["route"] = if selected.is_relay() {
                "relay"
            } else {
                "direct"
            }
            .into();
            fields["rtt_ms"] = millis(selected.rtt()).into();
        }
        if let Some(at_stall) = at_stall.filter(|_| !matches!(change, Change::Stalled(_))) {
            fields["tx_during_stall"] = (stats
                .udp_tx
                .datagrams
                .saturating_sub(at_stall.udp_tx.datagrams))
            .into();
            fields["lost_during_stall"] =
                (stats.lost_packets.saturating_sub(at_stall.lost_packets)).into();
        }
        (self.inner.log)("remote_connection_receive", fields);
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "receive_watch_tests.rs"]
mod tests;
