use std::path::{Path, PathBuf};

use super::{atomic, LockAcquireOutcome, RecoveryClassification, RuntimeLock, RuntimeLockError};

#[derive(Debug)]
pub struct StorageOpenGuard {
    marker_path: PathBuf,
    _writer_guard: RuntimeLock,
}

impl StorageOpenGuard {
    pub fn open(root: &Path) -> Result<StorageOpenOutcome, StorageOpenError> {
        let runtime_dir = root.join(".openaide-runtime");
        std::fs::create_dir_all(&runtime_dir)?;
        let writer_guard = match RuntimeLock::acquire(runtime_dir.join("storage-writer.lock"))? {
            LockAcquireOutcome::Acquired(lock) => lock,
            LockAcquireOutcome::Busy { .. } => return Err(StorageOpenError::LockedByLiveServer),
        };

        let marker_path = runtime_dir.join("storage-state.json");
        let recovery = read_open_recovery(&marker_path)?;
        write_marker(&marker_path, StorageOpenState::Open)?;

        Ok(StorageOpenOutcome {
            recovery,
            guard: StorageOpenGuard {
                marker_path,
                _writer_guard: writer_guard,
            },
        })
    }

    pub fn mark_clean_shutdown(&self) -> Result<(), std::io::Error> {
        write_marker(&self.marker_path, StorageOpenState::Clean)
    }
}

#[derive(Debug)]
pub struct StorageOpenOutcome {
    pub recovery: RecoveryClassification,
    pub guard: StorageOpenGuard,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageOpenError {
    #[error("storage locked by another live App Server")]
    LockedByLiveServer,
    #[error("storage schema version is incompatible: {found}")]
    IncompatibleSchema { found: u32 },
    #[error("storage lock failed: {0}")]
    RuntimeLock(#[from] RuntimeLockError),
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage metadata JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum StorageOpenState {
    Clean,
    Open,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageRuntimeMarker {
    schema_version: u32,
    state: StorageOpenState,
}

const STORAGE_RUNTIME_SCHEMA_VERSION: u32 = 1;

fn read_open_recovery(path: &Path) -> Result<RecoveryClassification, StorageOpenError> {
    if !path.exists() {
        return Ok(RecoveryClassification::CleanOpen);
    }
    let bytes = std::fs::read(path)?;
    let marker: StorageRuntimeMarker = serde_json::from_slice(&bytes)?;
    if marker.schema_version != STORAGE_RUNTIME_SCHEMA_VERSION {
        return Err(StorageOpenError::IncompatibleSchema {
            found: marker.schema_version,
        });
    }
    match marker.state {
        StorageOpenState::Clean => Ok(RecoveryClassification::CleanOpen),
        StorageOpenState::Open => Ok(RecoveryClassification::UncleanPreviousShutdown),
    }
}

fn write_marker(path: &Path, state: StorageOpenState) -> Result<(), std::io::Error> {
    let marker = StorageRuntimeMarker {
        schema_version: STORAGE_RUNTIME_SCHEMA_VERSION,
        state,
    };
    let bytes = serde_json::to_vec_pretty(&marker)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    atomic::write_bytes(path, &bytes)
}
