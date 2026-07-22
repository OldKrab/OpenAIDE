use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, RwLock};

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;
use crate::storage::records::TaskRecord;
use crate::storage::task_journal::catalog;
use crate::storage::task_journal::projection::replay_task;

use super::{failure, RecoveredTask};

type OpenedCatalog = (HashMap<String, TaskRecord>, HashMap<String, RecoveredTask>);

/// Opens the lightweight Navigation projection. Journals without a catalog are
/// replayed once for migration; cataloged Tasks stay unloaded until accessed.
pub(super) fn open_catalog(tasks_root: &Path) -> Result<OpenedCatalog, RuntimeError> {
    let mut records = HashMap::new();
    let mut recovered = HashMap::new();
    for entry in std::fs::read_dir(tasks_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let task_dir = entry.path();
        let task_id = entry.file_name().to_string_lossy().to_string();
        validate_task_id(&task_id)?;
        if failure::is_quarantined(&task_dir)? {
            recovered.insert(
                task_id,
                RecoveredTask::Unavailable {
                    error: "Task storage is quarantined after a durability failure".to_string(),
                },
            );
            continue;
        }
        // Journals created before fail-closed status bytes must gain the marker
        // even when their new Navigation catalog lets startup skip replay.
        failure::ensure_status(&task_dir)?;
        match catalog::load(&task_dir) {
            Ok(Some(entry)) if entry.task.task_id == task_id => {
                records.insert(task_id, entry.task);
                continue;
            }
            Ok(Some(_)) => crate::logging::warn(
                "task_catalog_identity_mismatch",
                serde_json::json!({ "task_id": task_id }),
            ),
            Ok(None) => {}
            Err(error) => crate::logging::warn(
                "task_catalog_unavailable",
                serde_json::json!({ "task_id": task_id, "error": error.to_string() }),
            ),
        }
        let Some((replayed_id, task)) = replay_task(&task_dir)? else {
            continue;
        };
        match &task {
            RecoveredTask::Available { projection, .. } => {
                refresh_catalog_cache(&task_dir, &projection.task);
                records.insert(replayed_id, projection.task.clone());
            }
            RecoveredTask::Unavailable { .. } => {
                recovered.insert(replayed_id, task);
            }
        }
    }
    Ok((records, recovered))
}

pub(super) fn ensure_task_loaded(
    tasks_root: &Path,
    catalog_records: &RwLock<HashMap<String, TaskRecord>>,
    projections: &RwLock<HashMap<String, RecoveredTask>>,
    load_lock: &Mutex<()>,
    task_id: &str,
) -> Result<bool, RuntimeError> {
    if projections
        .read()
        .expect("Task journal projections poisoned")
        .contains_key(task_id)
    {
        return Ok(true);
    }
    if !catalog_records
        .read()
        .expect("Task catalog poisoned")
        .contains_key(task_id)
    {
        return Ok(false);
    }
    let _load = load_lock
        .lock()
        .expect("Task projection load lock poisoned");
    if projections
        .read()
        .expect("Task journal projections poisoned")
        .contains_key(task_id)
    {
        return Ok(true);
    }
    let task_dir = tasks_root.join(task_id);
    let Some((replayed_id, task)) = replay_task(&task_dir)? else {
        return Ok(false);
    };
    if let RecoveredTask::Available { projection, .. } = &task {
        // Loading repairs a cache that survived a crash between journal and
        // catalog publication without making the cache authoritative.
        refresh_catalog_cache(&task_dir, &projection.task);
        catalog_records
            .write()
            .expect("Task catalog poisoned")
            .insert(replayed_id.clone(), projection.task.clone());
    }
    projections
        .write()
        .expect("Task journal projections poisoned")
        .insert(replayed_id, task);
    Ok(true)
}

pub(super) fn publish_catalog(
    tasks_root: &Path,
    records: &RwLock<HashMap<String, TaskRecord>>,
    task_id: &str,
    task: &RecoveredTask,
) -> Result<(), RuntimeError> {
    let RecoveredTask::Available { projection, .. } = task else {
        return Err(RuntimeError::Storage(
            "Unavailable Task cannot publish Navigation metadata".to_string(),
        ));
    };
    refresh_catalog_cache(&tasks_root.join(task_id), &projection.task);
    records
        .write()
        .expect("Task catalog poisoned")
        .insert(task_id.to_string(), projection.task.clone());
    Ok(())
}

fn refresh_catalog_cache(task_dir: &Path, task: &TaskRecord) {
    if let Err(error) = catalog::publish(task_dir, task) {
        // The journal is authoritative. A missing or stale cache stamp forces
        // a one-Task rebuild on the next startup; it must never invalidate an
        // already durable Task commit.
        crate::logging::warn(
            "task_catalog_publish_failed",
            serde_json::json!({
                "task_id": task.task_id,
                "error": error.to_string(),
            }),
        );
    }
}
