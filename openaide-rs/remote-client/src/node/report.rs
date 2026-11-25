//! What a dial records: one `remote_connect_completed` line per attempt that says,
//! on its own, which computer was dialled, for what, how its address was found,
//! what this device's network looked like, how far the attempt got and what ended
//! it. A shared report can then be read without matching lines up by attempt.
//!
//! Metadata only: fixed words, counts, and the first characters of the computer's
//! public key. An error's text is never written; its variant names it.

use serde_json::{json, Value};

/// The transport this device is online through, as its platform reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// No network at all.
    None,
    /// The platform could not say.
    Unknown,
    Wifi,
    Cellular,
    Ethernet,
    /// A VPN whose underlying transport the platform did not name.
    Vpn,
    Other,
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Unknown => "unknown",
            Self::Wifi => "wifi",
            Self::Cellular => "cellular",
            Self::Ethernet => "ethernet",
            Self::Vpn => "vpn",
            Self::Other => "other",
        }
    }
}

/// This device's network when it dialled: the transport it routes through,
/// whether the platform validated it as reaching the internet, and whether a VPN
/// carries it. Never an address or a network name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkKind {
    pub transport: Transport,
    pub validated: bool,
    pub vpn: bool,
}

impl NetworkKind {
    pub const NONE: Self = Self {
        transport: Transport::None,
        validated: false,
        vpn: false,
    };
    pub const UNKNOWN: Self = Self {
        transport: Transport::Unknown,
        validated: false,
        vpn: false,
    };

    /// Names the transport from the platform's flags; a VPN is reported beside the
    /// transport under it, when the platform says which.
    pub fn of(wifi: bool, cellular: bool, ethernet: bool, vpn: bool, validated: bool) -> Self {
        let transport = if wifi {
            Transport::Wifi
        } else if cellular {
            Transport::Cellular
        } else if ethernet {
            Transport::Ethernet
        } else if vpn {
            Transport::Vpn
        } else {
            Transport::Other
        };
        Self {
            transport,
            validated,
            vpn,
        }
    }

    pub(crate) fn add_fields(&self, fields: &mut Value) {
        fields["network"] = self.transport.as_str().into();
        fields["network_validated"] = self.validated.into();
        fields["vpn"] = self.vpn.into();
    }
}

/// Why the device dialled: the connection in use, a switch to another computer, or a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    Live,
    Switch,
    Check,
}

impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Switch => "switch",
            Self::Check => "check",
        }
    }
}

/// How far an attempt got. The stage of a failed attempt is the one that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Bind,
    Address,
    Connect,
    RefusalCheck,
    Done,
}

impl Stage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::Address => "address",
            Self::Connect => "connect",
            Self::RefusalCheck => "refusal_check",
            Self::Done => "done",
        }
    }
}

/// One attempt to reach a paired computer, filled in as the attempt goes.
#[derive(Debug, Clone)]
pub(crate) struct DialReport {
    purpose: Purpose,
    index: usize,
    target: String,
    network: NetworkKind,
    pub stage: Stage,
    /// `remembered_relay` when the stored relay was tried first, `lookup` when the
    /// address was looked up, `own_relay` when the computer was dialled at the Own
    /// Relay it was paired through.
    pub address: &'static str,
    /// Whether this device's own endpoint had a home relay when it dialled. Not
    /// about an Own Relay (ADR-0085); [`DialReport::computer_own_relay`] says that.
    pub own_relay: Option<bool>,
    /// Whether the computer was paired through an Own Relay (ADR-0085). Only whether:
    /// its address and password are never recorded.
    pub computer_own_relay: Option<bool>,
    /// What the endpoint knew of the computer's address when the attempt failed.
    pub known_relay: Option<bool>,
    pub known_direct: Option<usize>,
}

impl DialReport {
    pub fn new(purpose: Purpose, index: usize, server: &str, network: NetworkKind) -> Self {
        Self {
            purpose,
            index,
            target: short_id(server),
            network,
            stage: Stage::Bind,
            address: "none",
            own_relay: None,
            computer_own_relay: None,
            known_relay: None,
            known_direct: None,
        }
    }

    pub fn start_fields(&self, attempt: u32) -> Value {
        json!({
            "attempt": attempt,
            "purpose": self.purpose.as_str(),
            "target": self.target,
            "index": self.index,
        })
    }

    /// The terminal record's fields; `error_class` names an unexpected failure by
    /// its variant.
    pub fn fields(
        &self,
        attempt: u32,
        outcome: &str,
        duration_ms: u64,
        error_class: Option<&str>,
    ) -> Value {
        let mut fields = self.start_fields(attempt);
        fields["outcome"] = outcome.into();
        fields["duration_ms"] = duration_ms.into();
        self.network.add_fields(&mut fields);
        fields["stage"] = self.stage.as_str().into();
        fields["address"] = self.address.into();
        let presence = |present: bool| if present { "present" } else { "absent" };
        if let Some(own) = self.own_relay {
            fields["own_relay"] = presence(own).into();
        }
        if let Some(own) = self.computer_own_relay {
            fields["computer_own_relay"] = own.into();
        }
        if let Some(known) = self.known_relay {
            fields["known_relay"] = presence(known).into();
        }
        if let Some(direct) = self.known_direct {
            fields["known_direct"] = direct.into();
        }
        if let Some(class) = error_class {
            fields["error_class"] = class.into();
        }
        fields
    }
}

/// Enough of a public key to tell paired computers apart in one report.
pub(crate) fn short_id(id: &str) -> String {
    let short: String = id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    if short.is_empty() {
        "none".to_string()
    } else {
        short
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
