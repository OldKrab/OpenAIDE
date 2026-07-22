use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

use crate::protocol::errors::RuntimeError;
use crate::storage::records::TaskRecord;
use crate::storage::task_journal::frame::{FaultInjector, JournalKind};
use crate::storage::task_journal::model::TaskOperation;

use super::{
    compaction, freeze_shared_task, quarantine_or_abort, recovery, CompactionMode, RecoveredTask,
};
use crate::storage::task_journal::split;

/// Runs an explicit idle/forced compaction without mixing the physical storage
/// generation switch into the main semantic commit path.
pub(super) fn compact_loaded_task(
    tasks_root: &Path,
    catalog_records: &RwLock<HashMap<String, TaskRecord>>,
    projections: &RwLock<HashMap<String, RecoveredTask>>,
    task_id: &str,
    mut next_task: RecoveredTask,
    mode: CompactionMode,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    if split::exists(&tasks_root.join(task_id)) {
        let should_compact = mode == CompactionMode::Force
            || (mode == CompactionMode::IfWorthwhile
                && split::compaction_is_worthwhile(&tasks_root.join(task_id))?);
        if should_compact {
            let RecoveredTask::Available {
                projection,
                journal_sequence,
            } = &next_task
            else {
                return Err(RuntimeError::Storage("Task is unavailable".to_string()));
            };
            if let Err(error) = split::compact(
                &tasks_root.join(task_id),
                projection,
                *journal_sequence,
                faults,
            ) {
                quarantine_or_abort(tasks_root, task_id);
                return Err(freeze_shared_task(projections, task_id, error));
            }
        }
    } else {
        let compacted =
            match compaction::compact_task(tasks_root, &mut next_task, task_id, mode, faults) {
                Ok(compacted) => compacted,
                Err(error) => {
                    quarantine_or_abort(tasks_root, task_id);
                    return Err(freeze_shared_task(projections, task_id, error));
                }
            };
        if compacted {
            recovery::publish_catalog(tasks_root, catalog_records, task_id, &next_task)?;
        }
    }
    projections
        .write()
        .expect("Task journal projections poisoned")
        .insert(task_id.to_string(), next_task);
    Ok(())
}

/// Persists the split representation after the reducer has produced the exact
/// next projection, preserving Chat-before-metadata ordering in one place.
pub(super) fn persist_split_commit(
    task_dir: &Path,
    task: &RecoveredTask,
    operations: &[TaskOperation],
    storage_sequence: u64,
    has_artifact_reference: bool,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let RecoveredTask::Available { projection, .. } = task else {
        unreachable!("validated Task remains available")
    };
    if storage_sequence == 1 {
        split::publish_initial(task_dir, projection, storage_sequence, faults)
    } else {
        split::append(
            task_dir,
            projection,
            operations,
            storage_sequence,
            if has_artifact_reference {
                JournalKind::ArtifactReference
            } else {
                JournalKind::Task
            },
            faults,
        )
    }
}
