//! What an App Shell on a computer tells its user about pairing and a paired
//! computer (ADR-0081). The Desktop App and the Web App share this wording; the
//! Mobile App words its own for a phone, which scans codes instead of pasting them.

use openaide_remote_core::PairingCodeError;

use crate::Failure;

/// Why a text will not be spent as a code: a stable reason to log, and the
/// sentence for the user.
pub fn refused_code(error: PairingCodeError) -> (&'static str, &'static str) {
    match error {
        PairingCodeError::FromOlderOpenAide => (
            "from_older_openaide",
            "This code comes from an older OpenAIDE. Update OpenAIDE on that device, then make a code on the computer it should use: Settings › Remote › Add a device.",
        ),
        PairingCodeError::Malformed => ("malformed", "The code is incomplete. Copy it again."),
        PairingCodeError::NotAPairingCode => ("not_a_code", "This isn't an OpenAIDE code."),
    }
}

/// A code made by the computer in use, `name`, was meant for another device.
pub fn own_code(name: &str) -> String {
    format!("This is {name}'s own code. Enter it on the device that should use {name}.")
}

/// Why spending a code failed.
pub fn pairing_failed(failure: Failure) -> String {
    match failure {
        Failure::NotTrusted => {
            "This code was already used or has expired. Make a new one on the other computer."
                .to_string()
        }
        other => unusable(other, "The other computer"),
    }
}

/// Why the paired computer `name` cannot be used now, and what to do.
pub fn unusable(failure: Failure, name: &str) -> String {
    match failure {
        Failure::Unreachable => format!(
            "{name} is unreachable. Make sure OpenAIDE is open on it and both computers are online."
        ),
        Failure::NotTrusted => {
            format!("This computer was removed from {name}. Pair with it again in Settings.")
        }
        Failure::Incompatible => format!(
            "OpenAIDE on this computer and on {name} are different versions. Update both, then try again."
        ),
    }
}
