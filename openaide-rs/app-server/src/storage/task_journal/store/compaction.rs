use std::fs;
use std::path::Path;

use crate::protocol::errors::RuntimeError;

use super::{journal_path, RecoveredTask};
use crate::storage::task_journal::frame;
use crate::storage::task_journal::model::{CompactionMode, JournalFrame, TaskOperation};
use crate::storage::task_journal::scheduler::QueuedWrite;

// The release benchmark showed 40 incident batches reclaiming only ~6 KiB on
// a 4 MiB Task. Avoid that churn; compact after sustained frame growth or when
// a smaller journal can reclaim at least one meaningful write batch.
const OBSOLETE_FRAME_THRESHOLD: usize = 128;
const BYTE_RATIO: u64 = 2;
const MIN_RECLAIM_BYTES: u64 = 64 * 1024;

pub(super) fn compact_task(
    tasks_root: &Path,
    task: &mut RecoveredTask,
    task_id: &str,
    mode: CompactionMode,
    faults: &frame::FaultInjector,
) -> Result<bool, RuntimeError> {
    if mode == CompactionMode::None {
        return Ok(false);
    }
    let projection = match task {
        RecoveredTask::Available { projection, .. } => projection.clone(),
        RecoveredTask::Unavailable { error } => {
            return Err(RuntimeError::Storage(error.clone()));
        }
    };
    let compacted_frame = JournalFrame {
        format_version: 1,
        sequence: 1,
        operations: vec![TaskOperation::Create { projection }],
    };
    let journal = journal_path(tasks_root, task_id)?;
    if mode == CompactionMode::IfWorthwhile && !is_worthwhile(&journal, &compacted_frame)? {
        crate::logging::info(
            "task_journal_compaction_skipped",
            serde_json::json!({ "task_id": task_id }),
        );
        return Ok(false);
    }
    frame::replace_with_faults(&journal, &compacted_frame, faults)?;
    let RecoveredTask::Available {
        journal_sequence, ..
    } = task
    else {
        unreachable!("Task availability checked before compaction")
    };
    *journal_sequence = 1;
    crate::logging::info(
        "task_journal_compacted",
        serde_json::json!({ "task_id": task_id }),
    );
    Ok(true)
}

pub(super) fn requested_compaction(batch: &[QueuedWrite]) -> CompactionMode {
    batch.iter().map(|queued| queued.write.compaction).fold(
        CompactionMode::None,
        |current, requested| match (current, requested) {
            (CompactionMode::Force, _) | (_, CompactionMode::Force) => CompactionMode::Force,
            (CompactionMode::IfWorthwhile, _) | (_, CompactionMode::IfWorthwhile) => {
                CompactionMode::IfWorthwhile
            }
            _ => CompactionMode::None,
        },
    )
}

fn is_worthwhile(journal: &Path, compacted_frame: &JournalFrame) -> Result<bool, RuntimeError> {
    let replayed: frame::ReplayedFrames<JournalFrame> = frame::replay(journal)?;
    let obsolete_frames = replayed.frames.len().saturating_sub(1);
    if obsolete_frames >= OBSOLETE_FRAME_THRESHOLD {
        return Ok(true);
    }
    let current_bytes = fs::metadata(journal)?.len();
    let compacted_bytes = frame::one_frame_file_len(compacted_frame)?;
    let reclaimable_bytes = current_bytes.saturating_sub(compacted_bytes);
    Ok(current_bytes >= compacted_bytes.saturating_mul(BYTE_RATIO)
        && reclaimable_bytes >= MIN_RECLAIM_BYTES)
}
