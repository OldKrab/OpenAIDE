//! Reads the Pairing Code an App Server shows to invite a new device.
//!
//! The App Server owns the format (`remote_devices/pairing_code.rs`); this is
//! the reading side a device needs, checked against it by the App Server's edge
//! tests.
//
// TODO: the App Server keeps its own copy of the invite reader for tests and of
// the label rules. Move the whole codec here and let the App Server depend on
// this crate once a second Rust client makes the duplication cost more than the
// extra dependency edge.

use data_encoding::BASE32_NOPAD;
use iroh::PublicKey;

const INVITE_PREFIX: &str = "OAI1";
const JOIN_PREFIX: &str = "OAJ1";
const KEY_LEN: usize = 32;
const INVITE_SECRET_LEN: usize = 16;
/// Labels are shown to the user, so they stay short enough for one row.
const MAX_LABEL_BYTES: usize = 64;

/// An invite: the App Server to reach and the single-use secret to present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteCode {
    pub server: PublicKey,
    pub secret: [u8; INVITE_SECRET_LEN],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PairingCodeError {
    #[error("this is not an OpenAIDE pairing code")]
    NotAPairingCode,
    #[error("this code adds a device to a computer; it is not an invite")]
    WrongKind,
    #[error("the pairing code is damaged; copy it again")]
    Malformed,
}

impl InviteCode {
    pub fn decode(code: &str) -> Result<Self, PairingCodeError> {
        // A pasted code may carry the spaces, dashes, or case a person or app added.
        let code: String = code
            .chars()
            .filter(|character| !character.is_whitespace() && *character != '-')
            .map(|character| character.to_ascii_uppercase())
            .collect();
        let Some(payload) = code.strip_prefix(INVITE_PREFIX) else {
            return Err(if code.starts_with(JOIN_PREFIX) {
                PairingCodeError::WrongKind
            } else {
                PairingCodeError::NotAPairingCode
            });
        };
        let bytes = BASE32_NOPAD
            .decode(payload.as_bytes())
            .map_err(|_| PairingCodeError::Malformed)?;
        if bytes.len() != KEY_LEN + INVITE_SECRET_LEN {
            return Err(PairingCodeError::Malformed);
        }
        let key: &[u8; KEY_LEN] = bytes[..KEY_LEN]
            .try_into()
            .map_err(|_| PairingCodeError::Malformed)?;
        Ok(Self {
            server: PublicKey::from_bytes(key).map_err(|_| PairingCodeError::Malformed)?,
            secret: bytes[KEY_LEN..]
                .try_into()
                .map_err(|_| PairingCodeError::Malformed)?,
        })
    }

    /// The secret as the pairing protocol carries it.
    pub(crate) fn secret_text(&self) -> String {
        BASE32_NOPAD.encode(&self.secret)
    }
}

/// Trims a self-reported label to what one row can show, on a character boundary.
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

#[cfg(test)]
#[path = "pairing_code_tests.rs"]
mod tests;
