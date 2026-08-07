use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde_json::json;

use crate::logging;
use crate::protocol::errors::RuntimeError;

use super::file::{create_trace_file, TraceFile};
use super::naming::{trace_enabled, TRACE_DIR_ENV, TRACE_ENV};
use super::retention::{prune, TracePolicy};
use super::AcpTraceStatus;

#[derive(Debug, Clone)]
pub struct AcpTraceState {
    inner: Arc<Mutex<AcpTraceInner>>,
}

#[derive(Debug)]
struct AcpTraceInner {
    enabled: bool,
    root: PathBuf,
    policy: TracePolicy,
    active_paths: HashSet<PathBuf>,
    retained_bytes: u64,
    pending_bytes: u64,
}

impl AcpTraceState {
    pub fn from_env(storage_root: &Path) -> Self {
        let root = std::env::var_os(TRACE_DIR_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| default_trace_root(storage_root));
        let state = Self {
            inner: Arc::new(Mutex::new(AcpTraceInner {
                enabled: trace_enabled(std::env::var(TRACE_ENV).ok().as_deref()),
                root,
                policy: TracePolicy::default(),
                active_paths: HashSet::new(),
                retained_bytes: 0,
                pending_bytes: 0,
            })),
        };
        state.initialize_retention();
        state
    }

    pub fn disabled(storage_root: &Path) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AcpTraceInner {
                enabled: false,
                root: default_trace_root(storage_root),
                policy: TracePolicy::default(),
                active_paths: HashSet::new(),
                retained_bytes: 0,
                pending_bytes: 0,
            })),
        }
    }

    #[cfg(test)]
    pub(super) fn disabled_with_policy(storage_root: &Path, policy: TracePolicy) -> Self {
        let state = Self {
            inner: Arc::new(Mutex::new(AcpTraceInner {
                enabled: false,
                root: default_trace_root(storage_root),
                policy,
                active_paths: HashSet::new(),
                retained_bytes: 0,
                pending_bytes: 0,
            })),
        };
        state.initialize_retention();
        state
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<AcpTraceStatus, RuntimeError> {
        let mut inner = self.inner.lock().expect("ACP trace state lock poisoned");
        if enabled {
            fs::create_dir_all(&inner.root)?;
            apply_retention(&mut inner)?;
        }
        inner.enabled = enabled;
        Ok(status_from_inner(&inner))
    }

    pub fn status(&self) -> AcpTraceStatus {
        let inner = self.inner.lock().expect("ACP trace state lock poisoned");
        status_from_inner(&inner)
    }

    pub(super) fn is_enabled(&self) -> bool {
        self.inner
            .lock()
            .expect("ACP trace state lock poisoned")
            .enabled
    }

    pub(super) fn open_trace_file(&self, task_id: &str, operation: &str) -> Option<TraceFile> {
        let mut inner = self.inner.lock().expect("ACP trace state lock poisoned");
        if !inner.enabled {
            return None;
        }
        if fs::create_dir_all(&inner.root)
            .and_then(|_| apply_retention(&mut inner))
            .is_err()
        {
            logging::warn("acp_trace_retention_failed", json!({}));
        }
        let trace_file =
            create_trace_file(&inner.root, task_id, operation, inner.policy.max_file_bytes)?;
        inner.active_paths.insert(trace_file.path().to_path_buf());
        Some(trace_file)
    }

    pub(super) fn reserve_bytes(&self, path: &Path, bytes: u64) -> bool {
        let mut inner = self.inner.lock().expect("ACP trace state lock poisoned");
        if !inner.active_paths.contains(path) {
            return false;
        }
        if inner.retained_bytes.saturating_add(bytes) > inner.policy.max_total_bytes
            && apply_retention(&mut inner).is_err()
        {
            logging::warn("acp_trace_retention_failed", json!({}));
            return false;
        }
        if inner.retained_bytes.saturating_add(bytes) > inner.policy.max_total_bytes {
            return false;
        }
        inner.retained_bytes = inner.retained_bytes.saturating_add(bytes);
        inner.pending_bytes = inner.pending_bytes.saturating_add(bytes);
        true
    }

    pub(super) fn commit_bytes(&self, bytes: u64) {
        let mut inner = self.inner.lock().expect("ACP trace state lock poisoned");
        inner.pending_bytes = inner.pending_bytes.saturating_sub(bytes);
    }

    pub(super) fn release_bytes(&self, bytes: u64) {
        let mut inner = self.inner.lock().expect("ACP trace state lock poisoned");
        inner.pending_bytes = inner.pending_bytes.saturating_sub(bytes);
        inner.retained_bytes = inner.retained_bytes.saturating_sub(bytes);
        // A failed write may still have reached disk partially. Reconcile while
        // active paths remain protected so the total-budget counter stays safe.
        if apply_retention(&mut inner).is_err() {
            logging::warn("acp_trace_retention_failed", json!({}));
        }
    }

    pub(super) fn close_trace_file(&self, path: &Path) {
        let mut inner = self.inner.lock().expect("ACP trace state lock poisoned");
        inner.active_paths.remove(path);
        if apply_retention(&mut inner).is_err() {
            logging::warn("acp_trace_retention_failed", json!({}));
        }
    }

    fn initialize_retention(&self) {
        let mut inner = self.inner.lock().expect("ACP trace state lock poisoned");
        // Retention owns old closed traces even while new trace capture is
        // disabled, so switching diagnostics off cannot preserve them forever.
        let result = if inner.enabled {
            fs::create_dir_all(&inner.root).and_then(|_| apply_retention(&mut inner))
        } else {
            apply_retention(&mut inner)
        };
        if result.is_err() {
            logging::warn("acp_trace_retention_failed", json!({}));
        }
    }
}

fn apply_retention(inner: &mut AcpTraceInner) -> std::io::Result<()> {
    let outcome = prune(
        &inner.root,
        &inner.active_paths,
        inner.policy,
        SystemTime::now(),
    )?;
    // A concurrent writer reserves before touching disk. Keep those bytes in
    // the budget even when this scan observes the pre-write file length.
    inner.retained_bytes = outcome.retained_bytes.saturating_add(inner.pending_bytes);
    if outcome.removed_files > 0 {
        logging::info(
            "acp_trace_retention_pruned",
            json!({
                "removed_count": outcome.removed_files,
                "removed_bytes": outcome.removed_bytes,
                "retained_bytes": inner.retained_bytes,
            }),
        );
    }
    Ok(())
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
