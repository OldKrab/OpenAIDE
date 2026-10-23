//! The client half of Remote Devices (ADR-0062) for App Shells written in Rust.
//!
//! A shell on another computer pairs with an App Server by Pairing Code, then
//! reaches it by public key. The shell's Frontend keeps speaking plain HTTP: the
//! [`Gateway`] listens on loopback and carries each request over one stream of
//! the connection [`RemoteClient`] keeps open. The App Server trusts the
//! connection's key, so no credential of the App Server ever reaches this side.
//!
//! SSH Computers (ADR-0069) need only the [`SshLauncher`]; the `pairing`
//! feature, on by default, adds the iroh client, so a shell that only launches
//! over SSH does not link iroh.

#[cfg(feature = "pairing")]
mod client;
#[cfg(feature = "pairing")]
mod gateway;
#[cfg(feature = "pairing")]
mod pairing_code;
mod ssh;

use std::sync::Arc;

/// Receives metadata-only lifecycle events: a stable name and safe fields.
pub type Logger = Arc<dyn Fn(&'static str, serde_json::Value) + Send + Sync>;

#[cfg(feature = "pairing")]
pub use client::{
    application_alpn, bind, device_key_from_text, device_key_text, new_device_key, redeem, Failure,
    PairedServer, RemoteClient,
};
#[cfg(feature = "pairing")]
pub use gateway::Gateway;
#[cfg(feature = "pairing")]
pub use iroh::{Endpoint, PublicKey, SecretKey};
#[cfg(feature = "pairing")]
pub use pairing_code::{clamp_label, InviteCode, PairingCodeError};
#[cfg(feature = "release-archive")]
pub use ssh::archive;
pub use ssh::{
    FetchBinary, LaunchError, Launched, Ready, RemoteOutput, ServerRelease, SshLauncher,
    SshTransport, Stage, StartMode, StopOutcome, TransportError,
};
