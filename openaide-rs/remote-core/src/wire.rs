//! The protocols that both ends of a Remote Devices connection speak, and the
//! messages of the pairing exchange (ADR-0062).

use data_encoding::BASE32_NOPAD;
use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};

/// Pairing is a protocol of its own, so a connection with an untrusted key can
/// never reach a request handler.
pub const PAIRING_ALPN: &[u8] = b"openaide/pair/1";
/// The largest pairing message either end reads.
pub const PAIRING_MESSAGE_LIMIT: usize = 4096;

/// The application protocol. The name carries the protocol's major version, so a
/// device and an App Server with different majors fail at the handshake rather
/// than on a request.
pub fn application_alpn() -> Vec<u8> {
    format!("openaide/app-server/{}", APP_SERVER_PROTOCOL_VERSION.major).into_bytes()
}

/// The codes the App Server closes a connection with, so a device can tell the
/// user why it lost access.
pub mod close {
    /// The connection's key is not trusted, or the invite secret it presented
    /// was refused.
    pub const UNTRUSTED: u32 = 1;
    /// The device was removed while it was connected.
    pub const REMOVED: u32 = 2;
    /// The connection asked for a protocol the App Server does not speak.
    pub const UNKNOWN_PROTOCOL: u32 = 3;
}

/// What a new device sends to spend an invite.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteRequest {
    /// The invite secret in base32, as it appears in the Pairing Code.
    pub secret: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl InviteRequest {
    /// The secret's bytes, or `None` if the text is not base32. Lowercase text is
    /// accepted, as in a code.
    pub fn secret_bytes(&self) -> Option<Vec<u8>> {
        BASE32_NOPAD
            .decode(self.secret.to_ascii_uppercase().as_bytes())
            .ok()
    }
}

/// The outcome of a pairing, sent by the App Server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingResult {
    pub trusted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
