use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::logging;
use crate::protocol::errors::RuntimeError;

const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(30);

/// Spaces the relaunches of an Agent whose process keeps exiting before `initialize`.
/// Every demand would otherwise start another doomed process. The first exit waits for
/// nothing: the launcher's cache repair expects the next demand to retry at once.
#[derive(Default)]
pub(super) struct EarlyExitBackoff {
    agents: Mutex<HashMap<String, EarlyExits>>,
}

struct EarlyExits {
    count: u32,
    retry_at: Instant,
}

/// A launch held back until the Agent's next allowed attempt.
struct LaunchHeldBack {
    early_exits: u32,
    wait: Duration,
}

impl EarlyExitBackoff {
    /// Refuses a launch while the Agent's wait lasts; its callers already handle not-ready.
    pub(super) fn admit(&self, agent_id: &str, now: Instant) -> Result<(), RuntimeError> {
        let Some(held) = self.held_back(agent_id, now) else {
            return Ok(());
        };
        logging::warn(
            "acp_agent_launch_held_back",
            serde_json::json!({
                "operation": "agent/process_lifetime",
                "agent_id": agent_id,
                "early_exit_count": held.early_exits,
                "wait_ms": held.wait.as_millis(),
            }),
        );
        Err(RuntimeError::NotReady(
            "The Agent keeps exiting during startup. Try again shortly.".to_string(),
        ))
    }

    fn held_back(&self, agent_id: &str, now: Instant) -> Option<LaunchHeldBack> {
        let agents = self.agents.lock().expect("early exit backoff poisoned");
        let exits = agents.get(agent_id)?;
        (exits.retry_at > now).then(|| LaunchHeldBack {
            early_exits: exits.count,
            wait: exits.retry_at - now,
        })
    }

    pub(super) fn record_exit(&self, agent_id: &str, now: Instant) {
        let mut agents = self.agents.lock().expect("early exit backoff poisoned");
        let exits = agents.entry(agent_id.to_string()).or_insert(EarlyExits {
            count: 0,
            retry_at: now,
        });
        exits.count = exits.count.saturating_add(1);
        exits.retry_at = now + wait_after(exits.count);
    }

    /// The Agent started, or the user changed something: nothing predicts another exit.
    pub(super) fn clear(&self, agent_id: &str) {
        self.agents
            .lock()
            .expect("early exit backoff poisoned")
            .remove(agent_id);
    }
}

fn wait_after(exits: u32) -> Duration {
    match exits {
        0 | 1 => Duration::ZERO,
        exits => FIRST_WAIT
            .saturating_mul(1u32.checked_shl(exits - 2).unwrap_or(u32::MAX))
            .min(LONGEST_WAIT),
    }
}

#[cfg(test)]
#[path = "acp_early_exit_backoff_tests.rs"]
mod tests;
