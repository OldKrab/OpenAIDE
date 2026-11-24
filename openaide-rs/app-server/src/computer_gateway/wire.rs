//! The JSON lines the App Shell and the gateway process exchange over stdin and
//! stdout. Each request carries an id its response repeats; events carry none.

use serde::{Deserialize, Serialize};

use super::folder::Computer;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub call: Call,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "method",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Call {
    /// The paired Computers.
    Computers,
    /// Spends a Pairing Code. `in_use` names the Computer the asking browser uses,
    /// whose own code is refused (ADR-0081).
    Pair {
        code: String,
        in_use: Option<ComputerInUse>,
    },
    /// The local endpoint that carries requests to a paired Computer, started when
    /// none runs for it yet.
    Connect { computer: String },
    /// Stops using a Computer and removes it from the list.
    Forget { computer: String },
}

impl Call {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Computers => "computers",
            Self::Pair { .. } => "pair",
            Self::Connect { .. } => "connect",
            Self::Forget { .. } => "forget",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ComputerInUse {
    /// The App Server's public key, as its devices snapshot gives it.
    pub key: String,
    pub name: String,
}

/// Where a paired Computer is reached, in the shape a same-machine App Server
/// hands off, so the App Shell's proxy carries both alike.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub kind: &'static str,
    pub endpoint_url: String,
    pub auth_token: String,
}

/// Why a request did not succeed: a stable reason to log and route on, and a
/// sentence for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refusal {
    pub reason: &'static str,
    pub message: String,
}

impl Refusal {
    pub fn new(reason: &'static str, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Outgoing {
    Result { id: u64, result: serde_json::Value },
    Error { id: u64, error: Refusal },
    Event(Event),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum Event {
    /// The process reads requests.
    Ready,
    /// A paired Computer refused this device for good: it was removed, or the two
    /// sides run incompatible versions. Sent once per started connection.
    Failure {
        computer: String,
        reason: &'static str,
    },
}

pub fn computers_result(computers: Vec<Computer>) -> serde_json::Value {
    serde_json::json!({ "computers": computers })
}
