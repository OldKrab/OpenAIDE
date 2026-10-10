//! Pairing Codes: the plain text a QR code carries between two devices.
//!
//! An invite names the App Server and holds a single-use secret. A join request
//! names the new device and its self-reported labels. Both are base32 so a QR
//! code can use its compact alphanumeric mode and a user can paste them.

use data_encoding::BASE32_NOPAD;
use iroh::PublicKey;

const INVITE_PREFIX: &str = "OAI1";
const JOIN_PREFIX: &str = "OAJ1";
const KEY_LEN: usize = 32;
pub(crate) const INVITE_SECRET_LEN: usize = 16;
/// Labels are shown to the user, so they stay short enough for one row.
pub(crate) const MAX_LABEL_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InviteCode {
    pub server: PublicKey,
    pub secret: [u8; INVITE_SECRET_LEN],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JoinCode {
    pub device: PublicKey,
    pub name: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PairingCodeError {
    #[error("this is not an OpenAIDE pairing code")]
    NotAPairingCode,
    #[error("this code adds a device to a computer; open it on the new device")]
    WrongKind,
    #[error("the pairing code is damaged; read it again")]
    Malformed,
}

impl InviteCode {
    pub(crate) fn encode(&self) -> String {
        let mut bytes = Vec::with_capacity(KEY_LEN + INVITE_SECRET_LEN);
        bytes.extend_from_slice(self.server.as_bytes());
        bytes.extend_from_slice(&self.secret);
        format!("{INVITE_PREFIX}{}", BASE32_NOPAD.encode(&bytes))
    }

    #[cfg(test)]
    pub(crate) fn decode(code: &str) -> Result<Self, PairingCodeError> {
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
}

impl JoinCode {
    #[cfg(test)]
    pub(crate) fn encode(&self) -> String {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.device.as_bytes());
        push_label(&mut bytes, &self.name);
        push_label(&mut bytes, self.model.as_deref().unwrap_or(""));
        format!("{JOIN_PREFIX}{}", BASE32_NOPAD.encode(&bytes))
    }

    pub(crate) fn decode(code: &str) -> Result<Self, PairingCodeError> {
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

/// Trims a self-reported label to what one row can show, on a character boundary.
pub(crate) fn clamp_label(label: &str) -> String {
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
    // A pasted code may carry the spaces, dashes, or case a person or app added.
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

#[cfg(test)]
fn push_label(bytes: &mut Vec<u8>, label: &str) {
    let label = clamp_label(label);
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
