use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::protocol::errors::RuntimeError;

use super::artifact_path;
use crate::storage::task_journal::frame::{self, FaultInjector, ReplayedFrames};
use crate::storage::task_journal::model::{ArtifactFrame, ArtifactOperation, CompactionMode};

const SUPERSEDED_FRAME_THRESHOLD: usize = 128;
const BYTE_RATIO: u64 = 2;
const MIN_RECLAIM_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
pub(in crate::storage::task_journal) struct ArtifactCompactionOutcome {
    pub compacted_files: usize,
    pub removed_replacements: usize,
    pub reclaimed_bytes: u64,
}

/// Rewrites each committed artifact with the same frame sequence while dropping
/// only structured replacements fully superseded by its final replacement.
pub(in crate::storage::task_journal) fn compact_all(
    tasks_root: &Path,
    task_id: &str,
    artifact_heads: &HashMap<String, u64>,
    mode: CompactionMode,
    faults: &FaultInjector,
) -> Result<ArtifactCompactionOutcome, RuntimeError> {
    let mut artifacts = artifact_heads.iter().collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.0.cmp(right.0));
    let mut outcome = ArtifactCompactionOutcome::default();
    for (artifact_id, committed_head) in artifacts {
        let compacted = compact_one(
            tasks_root,
            task_id,
            artifact_id,
            *committed_head,
            mode,
            faults,
        )?;
        if let Some((removed_replacements, reclaimed_bytes)) = compacted {
            outcome.compacted_files += 1;
            outcome.removed_replacements += removed_replacements;
            outcome.reclaimed_bytes = outcome.reclaimed_bytes.saturating_add(reclaimed_bytes);
        }
    }
    Ok(outcome)
}

fn compact_one(
    tasks_root: &Path,
    task_id: &str,
    artifact_id: &str,
    committed_head: u64,
    mode: CompactionMode,
    faults: &FaultInjector,
) -> Result<Option<(usize, u64)>, RuntimeError> {
    if committed_head == 0 || mode == CompactionMode::None {
        return Ok(None);
    }
    let path = artifact_path(tasks_root, task_id, artifact_id)?;
    let replayed: ReplayedFrames<ArtifactFrame> = frame::replay(&path)?;
    let committed_frames = usize::try_from(committed_head)
        .map_err(|_| RuntimeError::Storage("Tool artifact head exceeds memory".to_string()))?;
    if replayed.frames.len() < committed_frames {
        return Err(RuntimeError::Storage(format!(
            "Tool artifact {artifact_id} ends before committed sequence {committed_head}"
        )));
    }

    let physical_bytes = fs::metadata(&path)?.len();
    let mut frames = replayed.frames;
    frames.truncate(committed_frames);
    let replacement_count = frames
        .iter()
        .flat_map(|frame| &frame.operations)
        .filter(|operation| matches!(operation, ArtifactOperation::ReplaceDetails { .. }))
        .count();
    let removed_replacements = replacement_count.saturating_sub(1);
    let mut replacement_index = 0;
    for frame in &mut frames {
        frame.operations.retain(|operation| {
            if matches!(operation, ArtifactOperation::ReplaceDetails { .. }) {
                replacement_index += 1;
                replacement_index == replacement_count
            } else {
                true
            }
        });
    }

    let compacted_bytes = frame::framed_file_len(&frames)?;
    let reclaimable_bytes = physical_bytes.saturating_sub(compacted_bytes);
    let worthwhile = match mode {
        CompactionMode::None => false,
        CompactionMode::Force => reclaimable_bytes > 0,
        CompactionMode::IfWorthwhile => {
            removed_replacements >= SUPERSEDED_FRAME_THRESHOLD
                || (physical_bytes >= compacted_bytes.saturating_mul(BYTE_RATIO)
                    && reclaimable_bytes >= MIN_RECLAIM_BYTES)
        }
    };
    if !worthwhile {
        return Ok(None);
    }

    frame::replace_frames_with_faults(&path, &frames, faults)?;
    Ok(Some((removed_replacements, reclaimable_bytes)))
}
