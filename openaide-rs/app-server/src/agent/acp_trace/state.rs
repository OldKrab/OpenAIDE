use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::protocol::errors::RuntimeError;

use super::naming::{trace_enabled, TRACE_DIR_ENV, TRACE_ENV};
use super::AcpTraceStatus;

#[derive(Debug, Clone)]
pub struct AcpTraceState {
    inner: Arc<Mutex<AcpTraceInner>>,
}

#[derive(Debug)]
struct AcpTraceInner {
    enabled: bool,
    root: PathBuf,
}

impl AcpTraceState {
    pub fn from_env(storage_root: &Path) -> Self {
        let root = std::env::var_os(TRACE_DIR_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| default_trace_root(storage_root));
        Self {
            inner: Arc::new(Mutex::new(AcpTraceInner {
                enabled: trace_enabled(std::env::var(TRACE_ENV).ok().as_deref()),
                root,
            })),
        }
    }

    pub fn disabled(storage_root: &Path) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AcpTraceInner {
                enabled: false,
                root: default_trace_root(storage_root),
            })),
        }
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<AcpTraceStatus, RuntimeError> {
        let mut inner = self.inner.lock().expect("ACP trace state lock poisoned");
        if enabled {
            fs::create_dir_all(&inner.root)?;
        }
        inner.enabled = enabled;
        Ok(status_from_inner(&inner))
    }

    pub fn status(&self) -> AcpTraceStatus {
        let inner = self.inner.lock().expect("ACP trace state lock poisoned");
        status_from_inner(&inner)
    }

    pub(super) fn enabled_root(&self) -> Option<PathBuf> {
        let inner = self.inner.lock().expect("ACP trace state lock poisoned");
        inner.enabled.then(|| inner.root.clone())
    }
}

fn default_trace_root(storage_root: &Path) -> PathBuf {
    storage_root.join("diagnostics").join("acp-traces")
}

fn status_from_inner(inner: &AcpTraceInner) -> AcpTraceStatus {
    AcpTraceStatus {
        enabled: inner.enabled,
        directory: inner.root.to_string_lossy().to_string(),
    }
}
