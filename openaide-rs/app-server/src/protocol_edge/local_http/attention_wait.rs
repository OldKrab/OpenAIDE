//! Holds a plain local HTTP `taskAttention/wait` while the client's cursor is current.
//!
//! The gateway answers synchronously under its protocol lock, so the hold lives here, on
//! the per-connection listener thread, before the request is dispatched. A Remote Device
//! reaches this through the device bridge, which forwards without a response deadline.
//! The hold re-reads the cursor on every delivery wake: committed Task updates publish
//! through the gateway, and publishing signals every waiting link.

use std::time::{Duration, Instant};

use openaide_app_server_protocol::methods::TASK_ATTENTION_WAIT;
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::protocol_edge::SharedRpcGateway;

/// How long one held wait may last. It stays below the client heartbeat timeout
/// (30 s) and matches the reliable-session poll, so intermediaries already see
/// requests of this length.
const HOLD_DEADLINE: Duration = Duration::from_secs(25);
/// Bounds staleness if some Task change is committed without signalling.
const FALLBACK_WAKE: Duration = Duration::from_secs(2);
/// A Task turn publishes a burst of updates; one read after the burst covers them.
const COALESCE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy)]
pub(super) struct AttentionWaitTiming {
    pub(super) deadline: Duration,
    pub(super) fallback_wake: Duration,
    pub(super) coalesce: Duration,
}

impl Default for AttentionWaitTiming {
    fn default() -> Self {
        Self {
            deadline: HOLD_DEADLINE,
            fallback_wake: FALLBACK_WAKE,
            coalesce: COALESCE,
        }
    }
}

/// Why a hold stopped; the dispatched request then answers with the current set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HoldEnd {
    /// The set of current Task Attention Events changed.
    Changed,
    /// The wait ran its full length with nothing to report.
    Deadline,
    /// The client is no longer initialized, or reading Tasks failed.
    Abandoned,
}

impl HoldEnd {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Deadline => "deadline",
            Self::Abandoned => "abandoned",
        }
    }
}

/// The cursor of a request that asks to be held: a `taskAttention/wait` with one.
pub(super) fn held_cursor(request: Option<&Value>) -> Option<&str> {
    let request = request?;
    if request.get("method").and_then(Value::as_str) != Some(TASK_ATTENTION_WAIT) {
        return None;
    }
    request.get("params")?.get("cursor")?.as_str()
}

/// Blocks while `cursor` still names the current set for this connection.
pub(super) fn hold_while_current(
    gateway: &SharedRpcGateway,
    connection_id: &ConnectionId,
    cursor: &str,
    timing: AttentionWaitTiming,
) -> HoldEnd {
    let signal = gateway.delivery_signal();
    let deadline = Instant::now() + timing.deadline;
    loop {
        // Snapshot before reading so a change committed meanwhile still wakes.
        let seen = signal.generation();
        match gateway.task_attention_cursor(connection_id, AppServerTime::now()) {
            None => return HoldEnd::Abandoned,
            Some(current) if current != cursor => return HoldEnd::Changed,
            Some(_) => {}
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return HoldEnd::Deadline;
        };
        if remaining.is_zero() {
            return HoldEnd::Deadline;
        }
        // TODO: wake on Task attention changes only. Every gateway delivery wakes this
        // hold, so a busy Desktop re-reads the Task list a few times a second for each
        // watching phone; a signal raised by the Task update publisher would avoid that.
        if signal.wait_changed(seen, timing.fallback_wake.min(remaining)) != seen {
            let remaining = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(timing.coalesce.min(remaining));
        }
    }
}

#[cfg(test)]
#[path = "attention_wait_tests.rs"]
mod tests;
