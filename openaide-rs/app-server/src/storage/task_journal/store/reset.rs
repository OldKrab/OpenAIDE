use std::fs::{self, File};
use std::sync::mpsc;

use crate::protocol::errors::RuntimeError;

use super::{CommitContext, TaskJournalStore};

const RESET_TOMBSTONE_PREFIX: &str = ".tasks.reset-";

impl TaskJournalStore {
    /// Drains earlier commits and replaces only the durable Task-history root.
    pub fn reset_task_history(&self) -> Result<(), RuntimeError> {
        let (reply, receiver) = mpsc::channel();
        self.inner.scheduler.request_reset(reply)?;
        receiver.recv().map_err(|_| {
            RuntimeError::Storage("Task journal worker stopped during history reset".to_string())
        })?
    }
}

pub(super) fn clear(context: &CommitContext) -> Result<(), RuntimeError> {
    let _load = context
        .projection_load_lock
        .lock()
        .expect("Task projection load lock poisoned");
    let mut artifact_heads = context
        .artifact_heads
        .lock()
        .expect("Task artifact reconciliation poisoned");
    let store_root = context
        .tasks_root
        .parent()
        .ok_or_else(|| RuntimeError::Storage("Task storage root has no parent".to_string()))?;
    let tombstone = store_root.join(format!("{RESET_TOMBSTONE_PREFIX}{}", uuid::Uuid::new_v4()));
    fs::rename(&context.tasks_root, &tombstone)?;
    if let Err(error) = super::super::frame::create_directory_durably(
        &context.tasks_root,
        super::super::frame::JournalKind::Root,
        context.faults.as_ref(),
    ) {
        if context.tasks_root.exists() {
            fs::remove_dir_all(&context.tasks_root).map_err(|rollback_error| {
                RuntimeError::Storage(format!(
                    "{error}; Task history reset rollback could not remove the replacement root: {rollback_error}"
                ))
            })?;
        }
        fs::rename(&tombstone, &context.tasks_root).map_err(|rollback_error| {
            RuntimeError::Storage(format!(
                "{error}; Task history reset rollback could not restore the original root: {rollback_error}"
            ))
        })?;
        return Err(error);
    }

    context
        .catalog
        .write()
        .expect("Task catalog poisoned")
        .clear();
    context
        .epoch_task_overlays
        .write()
        .expect("Task epoch overlays poisoned")
        .clear();
    context
        .projections
        .write()
        .expect("Task journal projections poisoned")
        .clear();
    artifact_heads.clear();

    remove_tombstone(&tombstone);
    Ok(())
}

pub(super) fn cleanup_tombstones(store_root: &std::path::Path) {
    let Ok(entries) = fs::read_dir(store_root) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(RESET_TOMBSTONE_PREFIX)
        {
            remove_tombstone(&entry.path());
        }
    }
}

fn remove_tombstone(path: &std::path::Path) {
    if let Err(error) = fs::remove_dir_all(path) {
        crate::logging::warn(
            "task_history_reset_cleanup_failed",
            serde_json::json!({ "error": error.to_string() }),
        );
        return;
    }
    if let Some(parent) = path.parent() {
        if let Err(error) = File::open(parent).and_then(|directory| directory.sync_all()) {
            crate::logging::warn(
                "task_history_reset_cleanup_sync_failed",
                serde_json::json!({ "error": error.to_string() }),
            );
        }
    }
}
