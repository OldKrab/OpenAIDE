//! Pairing Codes: the plain text a QR code carries between two devices.
//!
//! An invite names the App Server and holds a single-use secret. A join request
//! names the new device and the labels it reports for itself. Both codes are
//! base32, so a QR code can use its compact alphanumeric mode and a user can
//! paste the text.
//
// TODO(ADR-0081): remove `JoinCode` together with the join-request protocol
// methods in the next major version of the App Server Protocol.

use data_encoding::BASE32_NOPAD;
use iroh::PublicKey;

const INVITE_PREFIX: &str = "OAI1";
const JOIN_PREFIX: &str = "OAJ1";
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

/// A join request: the new device's key and the labels it reports for itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinCode {
    pub device: PublicKey,
    pub name: String,
    pub model: Option<String>,
}

/// Why a text is not a code of the expected kind. Each end words the message
/// for its own users, because only that end knows which action the user took.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PairingCodeError {
    #[error("this is not an OpenAIDE pairing code")]
    NotAPairingCode,
    /// A code of the other direction, such as an invite where a join request
    /// was expected.
    #[error("this is the other kind of pairing code")]
    WrongKind,
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
        let bytes = decode_payload(code, INVITE_PREFIX, JOIN_PREFIX)?;
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
}

impl JoinCode {
    pub fn encode(&self) -> String {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.device.as_bytes());
        push_label(&mut bytes, &self.name);
        push_label(&mut bytes, self.model.as_deref().unwrap_or(""));
        format!("{JOIN_PREFIX}{}", BASE32_NOPAD.encode(&bytes))
    }

    pub fn decode(code: &str) -> Result<Self, PairingCodeError> {
        let bytes = decode_payload(code, JOIN_PREFIX, INVITE_PREFIX)?;
        if bytes.len() < KEY_LEN {
            return Err(PairingCodeError::Malformed);
        }
        let device = public_key(&bytes[..KEY_LEN])?;
        let mut rest = &bytes[KEY_LEN..];
        let name = take_label(&mut rest)?;
        let model = take_label(&mut rest)?;
        if !rest.is_empty() || name.is_empty() {
            return Err(PairingCodeError::Malformed);
        }
        Ok(Self {
            device,
            name,
            model: Some(model).filter(|model| !model.is_empty()),
        })
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

fn decode_payload(
    code: &str,
    prefix: &str,
    other_prefix: &str,
) -> Result<Vec<u8>, PairingCodeError> {
    // A pasted code may carry spaces, dashes, or a change of case that a person or
    // an app added.
    let code: String = code
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .map(|character| character.to_ascii_uppercase())
        .collect();
    let Some(payload) = code.strip_prefix(prefix) else {
        return Err(if code.starts_with(other_prefix) {
            PairingCodeError::WrongKind
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

fn push_label(bytes: &mut Vec<u8>, label: &str) {
    let label = clamp_label(label);
    // `clamp_label` keeps a label within MAX_LABEL_BYTES, so its length fits in one byte.
    bytes.push(label.len() as u8);
    bytes.extend_from_slice(label.as_bytes());
}

fn take_label(rest: &mut &[u8]) -> Result<String, PairingCodeError> {
    let (&len, tail) = rest.split_first().ok_or(PairingCodeError::Malformed)?;
    let len = usize::from(len);
    if len > MAX_LABEL_BYTES || tail.len() < len {
        return Err(PairingCodeError::Malformed);
    }
    let label = std::str::from_utf8(&tail[..len]).map_err(|_| PairingCodeError::Malformed)?;
    *rest = &tail[len..];
    Ok(clamp_label(label))
}

#[cfg(test)]
#[path = "pairing_code_tests.rs"]
mod tests;
