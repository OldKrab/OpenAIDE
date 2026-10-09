//! Refreshes an Agent's Account Limits when no turn is there to report them: after an App
//! Server restart, or while the account is being used outside OpenAIDE.

use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use openaide_app_server_protocol::snapshot::AgentStatus;
use serde_json::json;

use crate::agent::gateway::AgentGateway;
use crate::agent::registry::CLAUDE_CODE_AGENT_ID;
use crate::agent::status_cache::AgentStatusCache;
use crate::logging;
use crate::protocol::errors::RuntimeError;

/// Clients ask whenever a Task is looked at; a reading or an attempt younger than this
/// answers them without another read.
pub(super) const MIN_READ_INTERVAL: Duration = Duration::from_secs(120);

#[cfg(test)]
#[path = "account_limits_refresh_tests.rs"]
mod tests;

/// Starts one background read unless a recent reading, a running read, or the Agent's state
/// makes it pointless. The read may launch the Agent process, so it never runs on the
/// caller's thread; its result is published through the status cache.
pub(super) fn start(
    gateway: &AgentGateway,
    statuses: &AgentStatusCache,
    agent_id: &str,
    min_interval: Duration,
) -> Option<JoinHandle<()>> {
    // TODO: replace this identity check with a capability the Agent advertises at
    // initialize, so a custom Agent built on the same adapter gets its limits refreshed too.
    if agent_id != CLAUDE_CODE_AGENT_ID {
        return None;
    }
    // Launching a process for an Agent that cannot answer would only disturb its setup.
    if matches!(
        statuses.snapshot(agent_id).status,
        AgentStatus::Installing
            | AgentStatus::SetupRequired
            | AgentStatus::AuthRequired
            | AgentStatus::Authenticating
            | AgentStatus::Unsupported
    ) {
        return None;
    }
    if !statuses.claim_account_limits_read(agent_id, min_interval) {
        return None;
    }
    let gateway = gateway.clone();
    let statuses = statuses.clone();
    let agent_id = agent_id.to_string();
    Some(std::thread::spawn(move || {
        let started_at = Instant::now();
        logging::info(
            "agent_account_limits_refresh_started",
            json!({ "agent_id": agent_id }),
        );
        let result = gateway.read_account_limits(&agent_id);
        let outcome = match &result {
            Ok(Some(_)) => "read",
            Ok(None) => "unavailable",
            Err(RuntimeError::MethodNotFound(_) | RuntimeError::CapabilityMissing(_)) => {
                "unsupported"
            }
            Err(_) => "failed",
        };
        match result {
            Ok(Some(change)) => statuses.record_account_limits(&agent_id, change),
            Ok(None) => {}
            Err(error) => {
                if outcome == "unsupported" {
                    statuses.record_account_limits_unsupported(&agent_id);
                }
                logging::warn(
                    "agent_account_limits_refresh_failed",
                    json!({
                        "agent_id": agent_id,
                        "outcome": outcome,
                        "duration_ms": started_at.elapsed().as_millis(),
                        "error_kind": error.reason(),
                    }),
                );
                return;
            }
        }
        logging::info(
            "agent_account_limits_refresh_completed",
            json!({
                "agent_id": agent_id,
                "outcome": outcome,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
    }))
}
