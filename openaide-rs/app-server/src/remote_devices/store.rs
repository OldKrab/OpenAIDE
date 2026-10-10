//! Durable trust for Remote Devices: the App Server key pair and the trusted device keys.

use std::path::{Path, PathBuf};

use iroh::SecretKey;
use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;

const FILE_NAME: &str = "remote-devices.json";

/// The file holds the App Server's private key, so it is readable by its owner only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredRemoteDevices {
    /// Created with the first Pairing Code. Dropped with the last Remote Device,
    /// so a later pairing starts from an identity no removed device has seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<StoredRemoteDevice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredRemoteDevice {
    pub device_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub added_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at_ms: Option<u64>,
}

impl StoredRemoteDevices {
    pub(crate) fn secret_key(&self) -> Option<SecretKey> {
        self.secret_key.as_deref()?.parse().ok()
    }

    pub(crate) fn ensure_secret_key(&mut self) -> SecretKey {
        if let Some(secret_key) = self.secret_key() {
            return secret_key;
        }
        let secret_key = SecretKey::generate();
        self.secret_key = Some(hex(&secret_key.to_bytes()));
        secret_key
    }

    pub(crate) fn forget_secret_key(&mut self) {
        self.secret_key = None;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteDeviceStore {
    path: PathBuf,
}

impl RemoteDeviceStore {
    pub(crate) fn new(state_root: &Path) -> Self {
        Self {
            path: state_root.join(FILE_NAME),
        }
    }

    /// A missing file means remote access was never used. An unreadable one is
    /// reported instead of replaced, so a damaged file cannot silently drop trust.
    pub(crate) fn read(&self) -> Result<StoredRemoteDevices, RuntimeError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(StoredRemoteDevices::default())
            }
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn write(&self, devices: &StoredRemoteDevices) -> Result<(), RuntimeError> {
        crate::storage::atomic::write_json(&self.path, devices)?;
        restrict_to_owner(&self.path)
    }
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> Result<(), RuntimeError> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(0o600),
    )?)
}

// The per-user profile directory that holds the state root is already private on Windows.
#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) -> Result<(), RuntimeError> {
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
