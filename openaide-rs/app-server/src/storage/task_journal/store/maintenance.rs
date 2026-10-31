use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Mutex, RwLock};
use std::time::Instant;

use crate::protocol::errors::RuntimeError;
use crate::storage::records::TaskRecord;
use crate::storage::task_journal::frame::{FaultInjector, JournalKind};
use crate::storage::task_journal::model::TaskOperation;

use super::{
    compaction, freeze_shared_task, quarantine_or_abort, recovery, CompactionMode, RecoveredTask,
};
use crate::storage::task_journal::{artifact, split};

/// Runs idle/forced storage maintenance without mixing physical rewrites into
/// the semantic commit path. It runs in the single storage worker on every completed turn, so
/// its cost delays every Task's writes.
#[allow(clippy::too_many_arguments)]
pub(super) fn compact_loaded_task(
    tasks_root: &Path,
    catalog_records: &RwLock<HashMap<String, TaskRecord>>,
    projections: &RwLock<HashMap<String, RecoveredTask>>,
    artifacts_written: &Mutex<artifact::ArtifactsWrittenSinceCheck>,
    task_id: &str,
    mut next_task: RecoveredTask,
    mode: CompactionMode,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let started = Instant::now();
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
    let RecoveredTask::Available { projection, .. } = &next_task else {
        return Err(RuntimeError::Storage("Task is unavailable".to_string()));
    };
    let written = artifacts_written
        .lock()
        .expect("Task artifact write tracking poisoned")
        .get(task_id)
        .cloned()
        .filter(|_| mode != CompactionMode::Force);
    let artifact_outcome = artifact::compact_artifacts(
        tasks_root,
        task_id,
        &projection.artifact_heads,
        written.as_ref(),
        mode,
        faults,
    )?;
    // A failed check keeps its candidates, so the next one retries them.
    artifacts_written
        .lock()
        .expect("Task artifact write tracking poisoned")
        .insert(task_id.to_string(), BTreeSet::new());
    if artifact_outcome.compacted_files > 0 {
        crate::logging::info(
            "tool_artifact_compacted",
            serde_json::json!({
                "task_id": task_id,
                "artifact_count": artifact_outcome.compacted_files,
                "removed_replacements": artifact_outcome.removed_replacements,
                "reclaimed_bytes": artifact_outcome.reclaimed_bytes,
            }),
        );
    }
    crate::logging::info(
        "task_compaction_checked",
        serde_json::json!({
            "operation": "task_compaction_check",
            "task_id": task_id,
            "artifact_scope": if written.is_some() { "written" } else { "all" },
            "checked_artifact_count": artifact_outcome.checked_files,
            "compacted_artifact_count": artifact_outcome.compacted_files,
            "duration_ms": started.elapsed().as_millis() as u64,
        }),
    );
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
