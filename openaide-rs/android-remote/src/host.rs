//! What the Kotlin shell keeps and observes for the node, and the bridge that
//! lets the node use it.

use std::sync::Arc;

use openaide_remote_client::{
    self as remote, Logger, NodeHost, PublicKey, SecretKey, StorageUnavailable,
};

/// The shell's secure storage was unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum StorageError {
    #[error("unavailable")]
    Unavailable,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for StorageError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Unavailable
    }
}

/// The transport Android routes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Transport {
    None,
    Unknown,
    Wifi,
    Cellular,
    Ethernet,
    Vpn,
    Other,
}

/// This phone's network: the transport, whether Android validated it as reaching
/// the internet, and whether a VPN carries it. Never an address or a network name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct NetworkKind {
    pub transport: Transport,
    pub validated: bool,
    pub vpn: bool,
}

impl From<remote::NetworkKind> for NetworkKind {
    fn from(kind: remote::NetworkKind) -> Self {
        let transport = match kind.transport {
            remote::Transport::None => Transport::None,
            remote::Transport::Unknown => Transport::Unknown,
            remote::Transport::Wifi => Transport::Wifi,
            remote::Transport::Cellular => Transport::Cellular,
            remote::Transport::Ethernet => Transport::Ethernet,
            remote::Transport::Vpn => Transport::Vpn,
            remote::Transport::Other => Transport::Other,
        };
        Self {
            transport,
            validated: kind.validated,
            vpn: kind.vpn,
        }
    }
}

impl From<NetworkKind> for remote::NetworkKind {
    fn from(kind: NetworkKind) -> Self {
        let transport = match kind.transport {
            Transport::None => remote::Transport::None,
            Transport::Unknown => remote::Transport::Unknown,
            Transport::Wifi => remote::Transport::Wifi,
            Transport::Cellular => remote::Transport::Cellular,
            Transport::Ethernet => remote::Transport::Ethernet,
            Transport::Vpn => remote::Transport::Vpn,
            Transport::Other => remote::Transport::Other,
        };
        Self {
            transport,
            validated: kind.validated,
            vpn: kind.vpn,
        }
    }
}

/// What the shell keeps for the node, and what only Android can observe. Called
/// on the library's threads; each call returns promptly.
#[uniffi::export(with_foreign)]
pub trait RemoteHost: Send + Sync {
    /// This phone's 32-byte key, or `None` before its first use.
    fn device_key(&self) -> Result<Option<Vec<u8>>, StorageError>;
    fn save_device_key(&self, key: Vec<u8>) -> Result<(), StorageError>;
    /// The id of the paired computer in use, or `None` when the phone uses none.
    fn computer_in_use(&self) -> Option<String>;
    /// The relay `computer` last answered through.
    fn relay(&self, computer: String) -> Option<String>;
    fn save_relay(&self, computer: String, relay: String) -> Result<(), StorageError>;
    fn network(&self) -> NetworkKind;
    /// One metadata-only lifecycle event; `fields` is a JSON object.
    fn log(&self, event: String, fields: String);
}

/// The node's view of the shell.
pub(crate) struct Bridge(pub Arc<dyn RemoteHost>);

impl NodeHost for Bridge {
    fn device_key(&self) -> Result<Option<SecretKey>, StorageUnavailable> {
        let Some(bytes) = self.0.device_key().map_err(|_| StorageUnavailable)? else {
            return Ok(None);
        };
        // A key that is not 32 bytes was damaged in storage; it cannot be replaced
        // silently, or every paired computer would stop trusting this phone.
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| StorageUnavailable)?;
        Ok(Some(SecretKey::from_bytes(&bytes)))
    }

    fn save_device_key(&self, key: &SecretKey) -> Result<(), StorageUnavailable> {
        self.0
            .save_device_key(key.to_bytes().to_vec())
            .map_err(|_| StorageUnavailable)
    }

    fn computer_in_use(&self) -> Option<PublicKey> {
        self.0.computer_in_use()?.parse().ok()
    }

    fn relay(&self, computer: &PublicKey) -> Option<String> {
        self.0.relay(computer.to_string())
    }

    fn save_relay(&self, computer: &PublicKey, relay: &str) -> Result<(), StorageUnavailable> {
        self.0
            .save_relay(computer.to_string(), relay.to_string())
            .map_err(|_| StorageUnavailable)
    }

    fn network(&self) -> remote::NetworkKind {
        self.0.network().into()
    }
}

/// The node's events, written to the shell's diagnostics log.
pub(crate) fn logger(host: Arc<dyn RemoteHost>) -> Logger {
    Arc::new(move |event, fields| host.log(event.to_string(), fields.to_string()))
}
