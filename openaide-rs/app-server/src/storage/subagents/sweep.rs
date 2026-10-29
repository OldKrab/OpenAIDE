//! A one-time background pass that upgrades Subagent histories nobody opens.
//!
//! Loading a history already upgrades what older writers left (see
//! [`super::history::load_upgraded_history`]), but a finished Subagent may never be
//! loaded again, so its journal of full-history frames would keep its disk space and
//! slow the image sweep that scans it. This pass finds such journals by size alone,
//! without reading them, and upgrades each through the same load.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use super::super::{Store, StoreInner};
use super::{load_upgraded_history, task_dir, HISTORY_JOURNAL_FILE, SUBAGENTS_DIR};
use crate::protocol::errors::RuntimeError;

/// A current journal holds at most `COMPACT_AFTER_FRAMES` frames of streamed text,
/// so a larger one is almost always a journal of full-history frames. The load
/// decides; the size only keeps the pass from reading small journals.
const UPGRADE_CANDIDATE_BYTES: u64 = 1 << 20;

impl Store {
    /// Starts the pass on its own thread. It holds the Store only while upgrading
    /// one history, and stops when the Store is dropped.
    pub fn start_subagent_history_sweep(&self) {
        let store = Arc::downgrade(&self.inner);
        let tasks_root = self.root().join("task-store-v1").join("tasks");
        let spawned = std::thread::Builder::new()
            .name("openaide-subagent-history-sweep".to_string())
            .spawn(move || sweep(&store, &tasks_root));
        if let Err(error) = spawned {
            crate::logging::warn(
                "subagent_history_sweep_failed",
                serde_json::json!({ "stage": "spawn", "error_kind": format!("{:?}", error.kind()) }),
            );
        }
    }

    /// Upgrades one history under the Subagent write lock, as a load would.
    fn upgrade_subagent_history(&self, history_dir: &Path) -> Result<bool, RuntimeError> {
        let _guard = self
            .inner
            .subagent_write_lock
            .lock()
            .expect("Subagent store lock poisoned");
        let subagent_root = history_dir.parent().unwrap_or(history_dir);
        let (_, upgraded) = load_upgraded_history(&task_dir(subagent_root), history_dir)?;
        Ok(upgraded)
    }
}

pub(super) fn sweep(store: &Weak<StoreInner>, tasks_root: &Path) {
    let started_at = std::time::Instant::now();
    let candidates = match upgrade_candidates(tasks_root) {
        Ok(candidates) => candidates,
        Err(error) => {
            crate::logging::warn(
                "subagent_history_sweep_failed",
                serde_json::json!({ "stage": "scan", "error_kind": format!("{:?}", error.kind()) }),
            );
            return;
        }
    };
    let (mut upgraded, mut failed) = (0usize, 0usize);
    for history_dir in &candidates {
        let Some(inner) = store.upgrade() else {
            crate::logging::info(
                "subagent_history_sweep_stopped",
                serde_json::json!({ "remaining_count": candidates.len() - upgraded - failed }),
            );
            return;
        };
        match (Store { inner }).upgrade_subagent_history(history_dir) {
            Ok(true) => upgraded += 1,
            Ok(false) => {}
            Err(error) => {
                failed += 1;
                crate::logging::warn(
                    "subagent_history_upgrade_failed",
                    serde_json::json!({
                        "task_id": history_dir.parent().and_then(Path::parent).and_then(Path::file_name).map(|name| name.to_string_lossy()),
                        "subagent_id": history_dir.file_name().map(|name| name.to_string_lossy()),
                        "error_kind": error.reason(),
                    }),
                );
            }
        }
    }
    // Quiet when there is nothing to upgrade, which is every start after the first.
    if !candidates.is_empty() {
        crate::logging::info(
            "subagent_history_sweep_completed",
            serde_json::json!({
                "candidate_count": candidates.len(),
                "upgraded_count": upgraded,
                "failed_count": failed,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
    }
}

/// History directories whose journal is large enough to hold full-history frames.
fn upgrade_candidates(tasks_root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    let tasks = match fs::read_dir(tasks_root) {
        Ok(tasks) => tasks,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidates),
        Err(error) => return Err(error),
    };
    for task in tasks {
        let Ok(subagents) = fs::read_dir(task?.path().join(SUBAGENTS_DIR)) else {
            continue;
        };
        for subagent in subagents {
            let history_dir = subagent?.path();
            let journal_bytes = fs::metadata(history_dir.join(HISTORY_JOURNAL_FILE))
                .map(|metadata| metadata.len())
                .unwrap_or_default();
            if journal_bytes >= UPGRADE_CANDIDATE_BYTES {
                candidates.push(history_dir);
            }
        }
    }
    Ok(candidates)
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
