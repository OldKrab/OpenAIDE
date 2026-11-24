//! What both ends of a Remote Devices connection must agree on (ADR-0062, ADR-0081).
//!
//! - The Pairing Code format.
//! - The pairing exchange on the wire.
//! - The network that endpoints bind on.
//!
//! The App Server and `openaide-remote-client` both build on this crate, so the two
//! ends cannot drift apart. It holds no connection logic: connecting, retrying, and
//! logging stay with each end.
//
// TODO(ADR-0081): the Mobile App still reads Pairing Codes in Kotlin
// (`apps/android/.../PairingCode.kt`). Move it onto this crate through its native
// bindings, so that one implementation remains.

mod network;
mod pairing_code;
mod wire;

pub use network::Network;
pub use pairing_code::{
    clamp_label, InviteCode, JoinCode, PairingCodeError, INVITE_SECRET_LEN, MAX_LABEL_BYTES,
};
pub use wire::{
    application_alpn, close, InviteRequest, PairingResult, PAIRING_ALPN, PAIRING_MESSAGE_LIMIT,
};
