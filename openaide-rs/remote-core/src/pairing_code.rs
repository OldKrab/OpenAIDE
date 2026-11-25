//! Pairing Codes: the plain text a QR code carries between two devices.
//!
//! A code is an invite (ADR-0081). It names the App Server and holds a
//! single-use secret. A computer on an Own Relay also puts that relay and its
//! password in the invite (ADR-0085), because no public lookup tells the device
//! where to find it. A code is base32, so a QR code can use its compact
//! alphanumeric mode and a user can paste the text. Older OpenAIDE versions
//! also showed join-request codes; those are only recognized, so the user can
//! be told where they came from.

use data_encoding::BASE32_NOPAD;
use iroh::PublicKey;

use crate::OwnRelay;

const INVITE_PREFIX: &str = "OAI1";
/// An invite that also carries an Own Relay. A version before ADR-0085 does not
/// read it, which is right: it could not reach that computer anyway.
const RELAY_INVITE_PREFIX: &str = "OAI2";
/// The prefix of a join-request code that an OpenAIDE before ADR-0081 showed.
const OLD_JOIN_PREFIX: &str = "OAJ1";
const KEY_LEN: usize = 32;
/// The length of an invite's single-use secret, in bytes.
pub const INVITE_SECRET_LEN: usize = 16;
/// Labels are shown to the user, so they stay short enough for one row.
pub const MAX_LABEL_BYTES: usize = 64;

/// An invite: the App Server to reach, the single-use secret to present, and the
/// Own Relay the App Server is on, if it uses one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteCode {
    pub server: PublicKey,
    pub secret: [u8; INVITE_SECRET_LEN],
    pub relay: Option<OwnRelay>,
}

/// Why a text is not an invite. Each end words the message for its own users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PairingCodeError {
    #[error("this is not an OpenAIDE pairing code")]
    NotAPairingCode,
    /// A join-request code, which only an older OpenAIDE shows. That device
    /// needs an update before it can use another computer.
    #[error("this code comes from an older OpenAIDE")]
    FromOlderOpenAide,
    #[error("the pairing code is damaged")]
    Malformed,
}

impl InviteCode {
    /// The invite as text. Without an Own Relay it is the `OAI1` code every version
    /// reads; with one, `OAI2` followed by the relay line after the key and secret.
    pub fn encode(&self) -> String {
        let mut bytes = Vec::with_capacity(KEY_LEN + INVITE_SECRET_LEN);
        bytes.extend_from_slice(self.server.as_bytes());
        bytes.extend_from_slice(&self.secret);
        let prefix = match &self.relay {
            None => INVITE_PREFIX,
            Some(relay) => {
                bytes.extend_from_slice(relay.line().as_bytes());
                RELAY_INVITE_PREFIX
            }
        };
        format!("{prefix}{}", BASE32_NOPAD.encode(&bytes))
    }

    pub fn decode(code: &str) -> Result<Self, PairingCodeError> {
        let (bytes, has_relay) = decode_payload(code)?;
        let fixed = KEY_LEN + INVITE_SECRET_LEN;
        let relay = match (has_relay, bytes.len()) {
            (false, length) if length == fixed => None,
            (true, length) if length > fixed => {
                let line = std::str::from_utf8(&bytes[fixed..])
                    .map_err(|_| PairingCodeError::Malformed)?;
                Some(OwnRelay::parse(line).map_err(|_| PairingCodeError::Malformed)?)
            }
            _ => return Err(PairingCodeError::Malformed),
        };
        Ok(Self {
            server: public_key(&bytes[..KEY_LEN])?,
            secret: bytes[KEY_LEN..fixed]
                .try_into()
                .map_err(|_| PairingCodeError::Malformed)?,
            relay,
        })
    }

    /// The secret in the form the pairing exchange carries it: base32, like the code.
    pub fn secret_text(&self) -> String {
        BASE32_NOPAD.encode(&self.secret)
    }

    /// Whether this code was made by the App Server whose key is `server_key`, in
    /// the text form the devices snapshot carries. A device compares it with the
    /// Computer it uses to tell the user they entered that Computer's own code
    /// (ADR-0081). Text that is not a key matches nothing.
    pub fn is_from(&self, server_key: &str) -> bool {
        server_key
            .parse::<PublicKey>()
            .is_ok_and(|key| key == self.server)
    }
}

/// Trims a self-reported label to what one row can show. The cut falls on a
/// character boundary, and control characters are dropped.
pub fn clamp_label(label: &str) -> String {
    let label = label.trim();
    let mut end = label.len().min(MAX_LABEL_BYTES);
    while !label.is_char_boundary(end) {
        end -= 1;
    }
    label[..end]
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

/// The decoded bytes, and whether the code is the kind that carries an Own Relay.
fn decode_payload(code: &str) -> Result<(Vec<u8>, bool), PairingCodeError> {
    // A pasted code may carry spaces, dashes, or a change of case that a person or
    // an app added.
    let code: String = code
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .map(|character| character.to_ascii_uppercase())
        .collect();
    let (payload, has_relay) = if let Some(payload) = code.strip_prefix(INVITE_PREFIX) {
        (payload, false)
    } else if let Some(payload) = code.strip_prefix(RELAY_INVITE_PREFIX) {
        (payload, true)
    } else {
        return Err(if code.starts_with(OLD_JOIN_PREFIX) {
            PairingCodeError::FromOlderOpenAide
        } else {
            PairingCodeError::NotAPairingCode
        });
    };
    BASE32_NOPAD
        .decode(payload.as_bytes())
        .map(|bytes| (bytes, has_relay))
        .map_err(|_| PairingCodeError::Malformed)
}

fn public_key(bytes: &[u8]) -> Result<PublicKey, PairingCodeError> {
    let bytes: &[u8; KEY_LEN] = bytes.try_into().map_err(|_| PairingCodeError::Malformed)?;
    PublicKey::from_bytes(bytes).map_err(|_| PairingCodeError::Malformed)
}

#[cfg(test)]
#[path = "pairing_code_tests.rs"]
mod tests;
