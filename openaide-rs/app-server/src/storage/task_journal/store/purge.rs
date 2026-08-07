use std::fs;
use std::sync::mpsc;

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;

use super::{CommitContext, TaskJournalStore};

const PURGE_TOMBSTONE_PREFIX: &str = ".task.purge-";

impl TaskJournalStore {
    /// Drains all earlier writes before physically reclaiming one hidden Task.
    pub(crate) fn purge_tombstoned_task(&self, task_id: &str) -> Result<(), RuntimeError> {
        validate_task_id(task_id)?;
        let (reply, receiver) = mpsc::channel();
        self.inner
            .scheduler
            .request_purge(task_id.to_string(), reply)?;
        receiver.recv().map_err(|_| {
            RuntimeError::Storage("Task journal worker stopped during Task purge".to_string())
        })?
    }
}

pub(super) fn tombstoned_task(context: &CommitContext, task_id: &str) -> Result<(), RuntimeError> {
    let _load = context
        .projection_load_lock
        .lock()
        .expect("Task projection load lock poisoned");
    let task = context
        .catalog
        .read()
        .expect("Task catalog poisoned")
        .get(task_id)
        .cloned()
        .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
    if !task.tombstoned {
        return Err(RuntimeError::Conflict(
            "Only a tombstoned Task can be physically purged".to_string(),
        ));
    }

    let store_root = context
        .tasks_root
        .parent()
        .ok_or_else(|| RuntimeError::Storage("Task storage root has no parent".to_string()))?;
    let task_dir = context.tasks_root.join(task_id);
    let tombstone = store_root.join(format!("{PURGE_TOMBSTONE_PREFIX}{}", uuid::Uuid::new_v4()));
    match fs::rename(&task_dir, &tombstone) {
        Ok(()) => sync_directory(&context.tasks_root)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    context
        .catalog
        .write()
        .expect("Task catalog poisoned")
        .remove(task_id);
    context
        .epoch_task_overlays
        .write()
        .expect("Task epoch overlays poisoned")
        .remove(task_id);
    context
        .projections
        .write()
        .expect("Task journal projections poisoned")
        .remove(task_id);
    context
        .artifact_heads
        .lock()
        .expect("Task artifact reconciliation poisoned")
        .retain(|(candidate_task_id, _), _| candidate_task_id != task_id);

    if tombstone.exists() {
        if let Err(error) = fs::remove_dir_all(&tombstone).and_then(|()| sync_directory(store_root))
        {
            // The canonical Task is already gone. Startup retries hidden
            // tombstone cleanup without restoring it to product history.
            crate::logging::warn(
                "task_retention_cleanup_failed",
                serde_json::json!({ "task_id": task_id, "error": error.to_string() }),
            );
        }
    }
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
            .starts_with(PURGE_TOMBSTONE_PREFIX)
        {
            if let Err(error) = fs::remove_dir_all(entry.path()) {
                crate::logging::warn(
                    "task_retention_cleanup_failed",
                    serde_json::json!({ "error": error.to_string() }),
                );
            }
        }
    }
}

#[cfg(unix)]
fn sync_directory(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
