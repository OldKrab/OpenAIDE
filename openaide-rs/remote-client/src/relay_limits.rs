//! Reports when a relay throttles this endpoint.
//!
//! The public relays limit each connection's rate without publishing the limit. Over the
//! limit, a relay stops reading from the connection, so traffic stalls instead of failing,
//! and a stall looks the same as a slow network. iroh counts the relay connections it was
//! told were throttled; this module logs each time that count grows, so a stalled session
//! can be matched to a throttled relay.

use std::time::Duration;

use iroh::Endpoint;

use crate::Logger;

/// How often the count is read. iroh offers no way to wait on it, and reading it is one
/// atomic load, so a quiet poll costs nothing worth noticing.
const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Logs `event` each time a relay throttles one of `endpoint`'s connections, until the
/// endpoint closes. A relay says so at most once per connection.
pub fn watch_relay_rate_limits(endpoint: &Endpoint, log: Logger, event: &'static str) {
    let endpoint = endpoint.clone();
    tokio::spawn(async move {
        let mut count = ThrottleCount::default();
        let mut timer = tokio::time::interval(POLL_INTERVAL);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let closed = endpoint.closed();
        tokio::pin!(closed);
        loop {
            tokio::select! {
                _ = &mut closed => break,
                _ = timer.tick() => {
                    let total = endpoint.metrics().socket.relay_conns_ratelimited.get();
                    if let Some(new) = count.observe(total) {
                        (log)(event, serde_json::json!({ "new": new, "total": total }));
                    }
                }
            }
        }
    });
}

/// The throttled-connection count last seen, so only growth is reported.
#[derive(Debug, Default)]
struct ThrottleCount {
    seen: u64,
}

impl ThrottleCount {
    /// How many connections were throttled since the last call, if any.
    fn observe(&mut self, total: u64) -> Option<u64> {
        let new = total.saturating_sub(self.seen);
        self.seen = self.seen.max(total);
        (new > 0).then_some(new)
    }
}

#[cfg(test)]
#[path = "relay_limits_tests.rs"]
mod tests;
