use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StateRootFingerprint(String);

impl StateRootFingerprint {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRoot {
    path: PathBuf,
    fingerprint: StateRootFingerprint,
}

impl StateRoot {
    pub fn resolve(path: impl AsRef<Path>) -> Result<Self, StateRootError> {
        let path = path.as_ref();
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let normalized = std::fs::canonicalize(&absolute).unwrap_or(absolute);
        let fingerprint = StateRootFingerprint(stable_path_fingerprint(&normalized));
        Ok(Self {
            path: normalized,
            fingerprint,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn fingerprint(&self) -> &StateRootFingerprint {
        &self.fingerprint
    }
}

#[derive(Debug, Error)]
pub enum StateRootError {
    #[error("failed to resolve current directory: {0}")]
    CurrentDirectory(#[from] std::io::Error),
}

fn stable_path_fingerprint(path: &Path) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
