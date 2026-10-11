//! The client half of Remote Devices (ADR-0062) for App Shells written in Rust.
//!
//! A shell on another computer pairs with an App Server by Pairing Code, then
//! reaches it by public key. The shell's Frontend keeps speaking plain HTTP: the
//! [`Gateway`] listens on loopback and carries each request over one stream of
//! the connection [`RemoteClient`] keeps open. The App Server trusts the
//! connection's key, so no credential of the App Server ever reaches this side.

mod client;
mod gateway;
mod pairing_code;

pub use client::{
    application_alpn, bind, device_key_from_text, device_key_text, new_device_key, redeem, Failure,
    Logger, PairedServer, RemoteClient,
};
pub use gateway::Gateway;
pub use iroh::{Endpoint, PublicKey, SecretKey};
pub use pairing_code::{clamp_label, InviteCode, PairingCodeError};
