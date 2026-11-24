//! Pairing Codes: the plain text a QR code carries between two devices.
//!
//! A code is an invite (ADR-0081). It names the App Server and holds a
//! single-use secret. It is base32, so a QR code can use its compact
//! alphanumeric mode and a user can paste the text. Older OpenAIDE versions
//! also showed join-request codes; those are only recognized, so the user can
//! be told where they came from.

use data_encoding::BASE32_NOPAD;
use iroh::PublicKey;

const INVITE_PREFIX: &str = "OAI1";
/// The prefix of a join-request code that an OpenAIDE before ADR-0081 showed.
const OLD_JOIN_PREFIX: &str = "OAJ1";
const KEY_LEN: usize = 32;
/// The length of an invite's single-use secret, in bytes.
pub const INVITE_SECRET_LEN: usize = 16;
/// Labels are shown to the user, so they stay short enough for one row.
pub const MAX_LABEL_BYTES: usize = 64;

/// An invite: the App Server to reach and the single-use secret to present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteCode {
    pub server: PublicKey,
    pub secret: [u8; INVITE_SECRET_LEN],
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
    pub fn encode(&self) -> String {
        let mut bytes = Vec::with_capacity(KEY_LEN + INVITE_SECRET_LEN);
        bytes.extend_from_slice(self.server.as_bytes());
        bytes.extend_from_slice(&self.secret);
        format!("{INVITE_PREFIX}{}", BASE32_NOPAD.encode(&bytes))
    }

    pub fn decode(code: &str) -> Result<Self, PairingCodeError> {
        let bytes = decode_payload(code)?;
        if bytes.len() != KEY_LEN + INVITE_SECRET_LEN {
            return Err(PairingCodeError::Malformed);
        }
        Ok(Self {
            server: public_key(&bytes[..KEY_LEN])?,
            secret: bytes[KEY_LEN..]
                .try_into()
                .map_err(|_| PairingCodeError::Malformed)?,
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

fn decode_payload(code: &str) -> Result<Vec<u8>, PairingCodeError> {
    // A pasted code may carry spaces, dashes, or a change of case that a person or
    // an app added.
    let code: String = code
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .map(|character| character.to_ascii_uppercase())
        .collect();
    let Some(payload) = code.strip_prefix(INVITE_PREFIX) else {
        return Err(if code.starts_with(OLD_JOIN_PREFIX) {
            PairingCodeError::FromOlderOpenAide
        } else {
            PairingCodeError::NotAPairingCode
        });
    };
    BASE32_NOPAD
        .decode(payload.as_bytes())
        .map_err(|_| PairingCodeError::Malformed)
}

fn public_key(bytes: &[u8]) -> Result<PublicKey, PairingCodeError> {
    let bytes: &[u8; KEY_LEN] = bytes.try_into().map_err(|_| PairingCodeError::Malformed)?;
    PublicKey::from_bytes(bytes).map_err(|_| PairingCodeError::Malformed)
}

#[cfg(test)]
#[path = "pairing_code_tests.rs"]
mod tests;
