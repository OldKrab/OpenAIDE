//! "Check the connection": the steps between this device and a paired computer,
//! each answered on its own, so a failure names the step that failed rather than
//! "can't reach your computer". The words a user reads stay with each App Shell.

use std::time::Instant;

use iroh::PublicKey;

use super::dial::route;
use super::report::{short_id, NetworkKind, Purpose, Transport};
use super::RemoteNode;
use crate::client::{elapsed_ms, Failure};

/// Whether this device's endpoint got a home relay, through which a computer
/// elsewhere answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckRelay {
    Reached,
    NotReached,
    NoEndpoint,
}

/// How the computer answered: the connection in use, a fresh dial by route, or a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckComputer {
    Connected,
    Relay,
    Direct,
    Unreachable,
    NotTrusted,
    Incompatible,
    NotChecked,
}

/// What each step of a check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckResult {
    pub network: NetworkKind,
    pub relay: CheckRelay,
    pub computer: CheckComputer,
}

impl CheckResult {
    /// The first step that failed, or `ok`; the outcome of the `remote_check_completed` record.
    pub fn outcome(&self) -> &'static str {
        if self.network.transport == Transport::None {
            return "offline";
        }
        if self.relay == CheckRelay::NoEndpoint {
            return "no_endpoint";
        }
        match self.computer {
            CheckComputer::Connected | CheckComputer::Relay | CheckComputer::Direct => "ok",
            _ if self.relay == CheckRelay::NotReached => "relay_not_reached",
            CheckComputer::Unreachable => "unreachable",
            CheckComputer::NotTrusted => "not_trusted",
            CheckComputer::Incompatible => "incompatible",
            CheckComputer::NotChecked => "not_checked",
        }
    }

    fn fields(&self) -> serde_json::Value {
        let mut fields = serde_json::json!({
            "outcome": self.outcome(),
            "relay": match self.relay {
                CheckRelay::Reached => "reached",
                CheckRelay::NotReached => "not_reached",
                CheckRelay::NoEndpoint => "no_endpoint",
            },
            "computer": match self.computer {
                CheckComputer::Connected => "connected",
                CheckComputer::Relay => "relay",
                CheckComputer::Direct => "direct",
                CheckComputer::Unreachable => "unreachable",
                CheckComputer::NotTrusted => "not_trusted",
                CheckComputer::Incompatible => "incompatible",
                CheckComputer::NotChecked => "not_checked",
            },
        });
        self.network.add_fields(&mut fields);
        fields
    }
}

impl RemoteNode {
    /// Runs the steps of a check: this device's network, a bound endpoint, its home
    /// relay, then `server`. The connection in use answers for the computer;
    /// otherwise one dial is made and its connection closed.
    pub async fn check(&self, server: PublicKey) -> CheckResult {
        let started = Instant::now();
        let target = short_id(&server.to_string());
        self.log(
            "remote_check_started",
            serde_json::json!({ "target": target }),
        );
        let network = self.inner.host.network();
        let timings = self.inner.timings;
        let endpoint = self.endpoint().await.ok();
        let relay = match &endpoint {
            None => CheckRelay::NoEndpoint,
            Some(endpoint) => {
                match tokio::time::timeout(timings.check_relay_wait, endpoint.online()).await {
                    Ok(()) => CheckRelay::Reached,
                    Err(_) => CheckRelay::NotReached,
                }
            }
        };
        let computer = if endpoint.is_none() {
            CheckComputer::NotChecked
        } else if self.live().is_some() {
            CheckComputer::Connected
        } else {
            match self
                .dial(server, 0, timings.check_dial, Purpose::Check)
                .await
            {
                Ok(connection) => {
                    let relayed = route(&connection) == "relay";
                    self.abandon(&connection);
                    if relayed {
                        CheckComputer::Relay
                    } else {
                        CheckComputer::Direct
                    }
                }
                Err(Failure::NotTrusted) => CheckComputer::NotTrusted,
                Err(Failure::Incompatible) => CheckComputer::Incompatible,
                Err(Failure::Unreachable | Failure::Storage) => CheckComputer::Unreachable,
            }
        };
        let result = CheckResult {
            network,
            relay,
            computer,
        };
        let mut fields = result.fields();
        fields["target"] = target.into();
        fields["duration_ms"] = elapsed_ms(started).into();
        self.log("remote_check_completed", fields);
        result
    }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
