use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::StateRootFingerprint;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEndpointRecord {
    pub server_id: String,
    pub state_root_fingerprint: String,
    pub pid: u32,
    pub protocol_version: String,
    pub app_version: String,
    pub status: RuntimeEndpointRecordStatus,
    pub auth_token: String,
    pub endpoints: Vec<RuntimeEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEndpoint {
    pub transport: TransportKind,
    pub address: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportKind {
    LocalHttp,
    Stdio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeEndpointRecordStatus {
    Starting,
    Running,
    Draining,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEndpointRecordWrite {
    pub record: RuntimeEndpointRecord,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct EndpointRecordStore {
    runtime_root: PathBuf,
}

impl EndpointRecordStore {
    pub fn new(runtime_root: impl Into<PathBuf>) -> Self {
        Self {
            runtime_root: runtime_root.into(),
        }
    }

    pub fn path_for(&self, fingerprint: &StateRootFingerprint) -> PathBuf {
        self.runtime_root
            .join(format!("{}.endpoint.json", fingerprint.as_str()))
    }

    pub fn read(
        &self,
        fingerprint: &StateRootFingerprint,
    ) -> Result<Option<RuntimeEndpointRecord>, EndpointRecordStoreError> {
        let path = self.path_for(fingerprint);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path)?;
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    pub fn write(
        &self,
        fingerprint: &StateRootFingerprint,
        record: &RuntimeEndpointRecord,
    ) -> Result<RuntimeEndpointRecordWrite, EndpointRecordStoreError> {
        if record.state_root_fingerprint != fingerprint.as_str() {
            return Err(EndpointRecordStoreError::FingerprintMismatch);
        }
        std::fs::create_dir_all(&self.runtime_root)?;
        let _lock = self.lock_record(fingerprint)?;
        let path = self.path_for(fingerprint);
        let bytes = serde_json::to_vec_pretty(record)?;
        write_protected(&path, &bytes)?;
        Ok(RuntimeEndpointRecordWrite {
            record: record.clone(),
            path,
        })
    }

    pub fn remove(
        &self,
        fingerprint: &StateRootFingerprint,
    ) -> Result<bool, EndpointRecordStoreError> {
        let _lock = self.lock_record(fingerprint)?;
        let path = self.path_for(fingerprint);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn remove_if(
        &self,
        fingerprint: &StateRootFingerprint,
        should_remove: impl FnOnce(&RuntimeEndpointRecord) -> bool,
    ) -> Result<bool, EndpointRecordStoreError> {
        let _lock = self.lock_record(fingerprint)?;
        let path = self.path_for(fingerprint);
        let current = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<RuntimeEndpointRecord>(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if !should_remove(&current) {
            return Ok(false);
        }
        match std::fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn is_runtime_path_under(&self, path: &Path) -> bool {
        path.starts_with(&self.runtime_root)
    }

    fn lock_record(
        &self,
        fingerprint: &StateRootFingerprint,
    ) -> Result<EndpointRecordLock, EndpointRecordStoreError> {
        std::fs::create_dir_all(&self.runtime_root)?;
        let path = self
            .runtime_root
            .join(format!("{}.endpoint.lock", fingerprint.as_str()));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.lock_exclusive()?;
        Ok(EndpointRecordLock { file })
    }
}

struct EndpointRecordLock {
    file: File,
}

impl Drop for EndpointRecordLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[derive(Debug, Error)]
pub enum EndpointRecordStoreError {
    #[error("endpoint record I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("endpoint record JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("endpoint record state-root fingerprint does not match record key")]
    FingerprintMismatch,
}

fn write_protected(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
