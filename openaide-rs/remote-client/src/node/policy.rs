//! When a [`RemoteNode`](super::RemoteNode) starts over, and how long it may take.
//! Kept free of the network so the rules can be tested.
//!
//! A phone's platform stops an app's network shortly after it leaves the screen,
//! and the endpoint cannot observe that: it keeps retrying a dead relay link at
//! growing intervals, so the next dial waits out whatever interval is running. A
//! new endpoint dials at once, which makes replacing it cheaper than waiting.

use std::time::Duration;

/// Every wait of a node. [`Timings::default`] is what devices use; tests shorten it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Timings {
    /// Shorter than the delay after which Android was seen to stop a hidden app's network.
    pub stale_after_hidden: Duration,
    /// All attempts of one connect share this; the user sees "Connecting…" throughout.
    pub connect_budget: Duration,
    /// A new endpoint connects in well under a second; later attempts allow for a slow network.
    pub attempt_timeouts: [Duration; 3],
    /// Too little left to finish a handshake is not worth another endpoint.
    pub shortest_attempt: Duration,
    /// The App Server completes the handshake before it checks the key, so a
    /// removed device learns it from the close that follows.
    pub refusal_wait: Duration,
    /// How long a failed dial asks its endpoint what it learned of the computer.
    pub known_address_wait: Duration,
    /// How long a check waits for this device's endpoint to reach its home relay.
    pub check_relay_wait: Duration,
    /// The one dial of a check.
    pub check_dial: Duration,
    /// Two missed heartbeats of the transport and then some: not a slow round trip.
    pub stall_after: Duration,
    /// How often the connection in use is sampled for received data.
    pub receive_sample: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            stale_after_hidden: Duration::from_millis(3_000),
            connect_budget: Duration::from_millis(24_000),
            attempt_timeouts: [
                Duration::from_millis(4_000),
                Duration::from_millis(8_000),
                Duration::from_millis(12_000),
            ],
            shortest_attempt: Duration::from_millis(1_000),
            refusal_wait: crate::client::REFUSAL_WAIT,
            known_address_wait: Duration::from_millis(500),
            check_relay_wait: Duration::from_millis(5_000),
            check_dial: Duration::from_millis(8_000),
            stall_after: Duration::from_millis(12_000),
            receive_sample: Duration::from_millis(2_000),
        }
    }
}

const RELAY_LIMIT: usize = 200;

impl Timings {
    pub fn stale_after_hidden(&self, hidden: Duration) -> bool {
        hidden > self.stale_after_hidden
    }

    /// How long attempt `index` (from zero) may take after `spent` of the budget,
    /// or `None` for no further attempt.
    pub fn attempt_timeout(&self, index: usize, spent: Duration) -> Option<Duration> {
        let limit = *self.attempt_timeouts.get(index)?;
        let remaining = self.connect_budget.checked_sub(spent)?;
        if remaining < self.shortest_attempt {
            return None;
        }
        Some(limit.min(remaining))
    }
}

/// Whether a failed dial gives the next attempt a new endpoint. Not while the
/// endpoint carries the connection in use, as when a switch to another computer
/// dials first: retiring it would end the computer the user is still on.
pub(crate) fn retire_after_failed_dial(carries_live_connection: bool) -> bool {
    !carries_live_connection
}

/// Whether an endpoint marked stale is given up now. Not while a switch is dialling
/// on it: that would end the switch's attempt, spending its budget on a reachable
/// computer or failing it outright when the app is hidden. The retire waits for the
/// dial to end.
pub(crate) fn retire_now(switch_dials_in_flight: u32) -> bool {
    switch_dials_in_flight == 0
}

/// The relay the computer was last reached through, which spares the first attempt
/// an address lookup. Later attempts dial by key alone, so an address that went out
/// of date cannot keep the computer unreachable.
pub(crate) fn relay_for(index: usize, remembered: Option<&str>) -> Option<&str> {
    remembered.filter(|relay| index == 0 && valid_relay(relay))
}

/// A relay worth keeping: an https URL of bounded length with nothing that could
/// break a stored value or a log line.
pub(crate) fn valid_relay(url: &str) -> bool {
    url.len() <= RELAY_LIMIT
        && url.starts_with("https://")
        && !url
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
