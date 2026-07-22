use std::collections::{hash_map::Entry, HashMap};
#[cfg(test)]
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;

use super::frame::{self, FaultInjector, JournalKind, ReplayedFrames};
use super::model::{
    ArtifactFrame, ArtifactOperation, CommittedArtifactChange, TerminalOutputAppend,
    ToolArtifactProjection,
};

const ARTIFACTS_DIR: &str = "artifacts";
pub(super) type ReconciledArtifactHeads = HashMap<(String, String), u64>;

/// Writes and syncs an artifact frame before its visibility head is referenced
/// by the Task journal. A crash can therefore leave only an invisible orphan.
#[cfg(test)]
pub(super) fn prepare(
    tasks_root: &Path,
    task_id: &str,
    artifact_id: &str,
    committed_head: u64,
    operations: Vec<ArtifactOperation>,
) -> Result<CommittedArtifactChange, RuntimeError> {
    prepare_with_faults(
        tasks_root,
        task_id,
        artifact_id,
        committed_head,
        operations,
        &FaultInjector::disabled(),
    )
}

#[cfg(test)]
pub(super) fn prepare_with_faults(
    tasks_root: &Path,
    task_id: &str,
    artifact_id: &str,
    committed_head: u64,
    operations: Vec<ArtifactOperation>,
    faults: &FaultInjector,
) -> Result<CommittedArtifactChange, RuntimeError> {
    let path = artifact_path(tasks_root, task_id, artifact_id)?;
    let reconciled_head = path.exists().then_some(committed_head);
    prepare_reconciled_with_faults(
        tasks_root,
        task_id,
        artifact_id,
        committed_head,
        reconciled_head,
        operations,
        faults,
    )
}

pub(super) fn prepare_reconciled_with_faults(
    tasks_root: &Path,
    task_id: &str,
    artifact_id: &str,
    committed_head: u64,
    reconciled_head: Option<u64>,
    operations: Vec<ArtifactOperation>,
    faults: &FaultInjector,
) -> Result<CommittedArtifactChange, RuntimeError> {
    let sequence = committed_head
        .checked_add(1)
        .ok_or_else(|| RuntimeError::Storage("Tool artifact sequence overflow".to_string()))?;
    let change = committed_change(artifact_id, sequence, &operations);
    let artifact_frame = ArtifactFrame {
        format_version: 1,
        sequence,
        operations,
    };
    let path = artifact_path(tasks_root, task_id, artifact_id)?;
    if path
        .try_exists()
        .map_err(|error| artifact_io_error("prepare_exists", error))?
    {
        if reconciled_head != Some(committed_head) {
            return Err(RuntimeError::Storage(
                "Tool artifact is unavailable after startup reconciliation".to_string(),
            ));
        }
        // Startup reconciliation already made the physical head equal to the
        // authoritative Task head. The sole writer either commits the next
        // reference or freezes this Task, so normal appends never need to
        // replay an artifact's unbounded lifetime history.
        frame::append_with_faults(&path, &artifact_frame, JournalKind::Artifact, faults)
            .map_err(|error| artifact_frame_error("prepare_append", task_id, artifact_id, error))?;
    } else if sequence == 1 {
        frame::create_with_faults(&path, &artifact_frame, JournalKind::Artifact, faults)
            .map_err(|error| artifact_frame_error("prepare_create", task_id, artifact_id, error))?;
    } else {
        return Err(RuntimeError::Storage(format!(
            "Tool artifact {artifact_id} is missing committed sequence {committed_head}"
        )));
    }
    Ok(change)
}

/// Replays only the artifact prefix made visible by the Task journal head.
pub(super) fn load(
    tasks_root: &Path,
    task_id: &str,
    artifact_id: &str,
    committed_head: u64,
) -> Result<ToolArtifactProjection, RuntimeError> {
    let path = artifact_path(tasks_root, task_id, artifact_id)?;
    let replayed: ReplayedFrames<ArtifactFrame> = frame::replay(&path)
        .map_err(|error| artifact_frame_error("load_replay", task_id, artifact_id, error))?;
    if replayed.frames.len() < committed_head as usize {
        return Err(RuntimeError::Storage(format!(
            "Tool artifact {artifact_id} ends before committed sequence {committed_head}"
        )));
    }
    let mut details = None;
    let mut terminal_order = Vec::new();
    let mut terminal_outputs = HashMap::<String, String>::new();
    for artifact_frame in replayed.frames.into_iter().take(committed_head as usize) {
        for operation in artifact_frame.operations {
            match operation {
                ArtifactOperation::ReplaceDetails {
                    details: replacement,
                } => details = Some(*replacement),
                ArtifactOperation::AppendTerminal { terminal_id, data } => {
                    match terminal_outputs.entry(terminal_id) {
                        Entry::Occupied(mut output) => output.get_mut().push_str(&data),
                        Entry::Vacant(output) => {
                            terminal_order.push(output.key().clone());
                            output.insert(data);
                        }
                    }
                }
            }
        }
    }
    Ok(ToolArtifactProjection {
        artifact_id: artifact_id.to_string(),
        revision: committed_head,
        details,
        terminal_order,
        terminal_outputs,
    })
}

/// Restores the crash-consistent physical head for one Tool detail at the
/// moment a caller first reads or extends it.
pub(super) fn reconcile_one(
    tasks_root: &Path,
    task_id: &str,
    artifact_id: &str,
    committed_head: u64,
) -> Result<u64, RuntimeError> {
    let path = artifact_path(tasks_root, task_id, artifact_id)?;
    if !path.try_exists()? {
        if committed_head == 0 {
            return Ok(0);
        }
        return Err(RuntimeError::Storage(format!(
            "Tool artifact {artifact_id} is missing committed sequence {committed_head}"
        )));
    }
    let replayed: ReplayedFrames<ArtifactFrame> = frame::scan(&path)
        .map_err(|error| artifact_frame_error("reconcile_replay", task_id, artifact_id, error))?;
    if replayed.frame_count < committed_head as usize {
        return Err(RuntimeError::Storage(format!(
            "Tool artifact {artifact_id} ends before committed sequence {committed_head}"
        )));
    }
    frame::truncate_after(&path, &replayed, committed_head as usize)
        .map_err(|error| artifact_frame_error("reconcile_truncate", task_id, artifact_id, error))?;
    Ok(committed_head)
}

/// Reconciles prepared artifact tails against Task visibility heads at open.
/// A corrupt or missing artifact is isolated to Tool detail and logged; the
/// containing Task projection remains readable.
#[cfg(test)]
pub(super) fn reconcile(
    tasks_root: &Path,
    tasks: &HashMap<String, super::store::RecoveredTask>,
) -> Result<ReconciledArtifactHeads, RuntimeError> {
    let mut reconciled_heads = HashMap::new();
    for entry in fs::read_dir(tasks_root)
        .map_err(|error| artifact_io_error("reconcile_tasks_read", error))?
    {
        let entry = entry.map_err(|error| artifact_io_error("reconcile_task_entry", error))?;
        if !entry
            .file_type()
            .map_err(|error| artifact_io_error("reconcile_task_type", error))?
            .is_dir()
        {
            continue;
        }
        let task_id = entry.file_name().to_string_lossy().to_string();
        let Some(super::store::RecoveredTask::Available { projection, .. }) = tasks.get(&task_id)
        else {
            // Without an authoritative Task head, every artifact byte is
            // diagnostic evidence. In particular, quarantined Tasks must not
            // reinterpret "unknown" as an empty committed prefix.
            continue;
        };
        let artifacts_dir = entry.path().join(ARTIFACTS_DIR);
        let entries = match fs::read_dir(&artifacts_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                crate::logging::warn(
                    "tool_artifact_directory_unavailable",
                    serde_json::json!({
                        "task_id": task_id,
                        "error_kind": format!("{:?}", error.kind()),
                    }),
                );
                continue;
            }
        };
        for artifact_entry in entries {
            let artifact_entry = artifact_entry
                .map_err(|error| artifact_io_error("reconcile_artifact_entry", error))?;
            if !artifact_entry
                .file_type()
                .map_err(|error| artifact_io_error("reconcile_artifact_type", error))?
                .is_file()
            {
                continue;
            }
            let path = artifact_entry.path();
            let Some(artifact_id) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".journal"))
            else {
                continue;
            };
            let committed_head = projection
                .artifact_heads
                .get(artifact_id)
                .copied()
                .unwrap_or_default();
            let replayed: ReplayedFrames<ArtifactFrame> = match frame::scan(&path) {
                Ok(replayed) => replayed,
                Err(_) => {
                    crate::logging::warn(
                        "tool_artifact_unavailable",
                        serde_json::json!({
                            "task_id": task_id,
                            "artifact_id": artifact_id,
                            "stage": "artifact_replay",
                            "error_kind": "framed_journal_replay_failed",
                        }),
                    );
                    continue;
                }
            };
            if replayed.frame_count < committed_head as usize {
                crate::logging::warn(
                    "tool_artifact_missing_committed_frame",
                    serde_json::json!({
                        "task_id": task_id,
                        "artifact_id": artifact_id,
                        "committed_head": committed_head,
                        "physical_head": replayed.frame_count,
                    }),
                );
                continue;
            }
            frame::truncate_after(&path, &replayed, committed_head as usize).map_err(|error| {
                artifact_frame_error("reconcile_truncate", &task_id, artifact_id, error)
            })?;
            reconciled_heads.insert((task_id.clone(), artifact_id.to_string()), committed_head);
        }
    }
    Ok(reconciled_heads)
}

pub(super) fn validate_artifact_id(artifact_id: &str) -> Result<(), RuntimeError> {
    if artifact_id.is_empty()
        || !artifact_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(RuntimeError::Storage(
            "Invalid Tool artifact id".to_string(),
        ));
    }
    Ok(())
}

fn artifact_path(
    tasks_root: &Path,
    task_id: &str,
    artifact_id: &str,
) -> Result<PathBuf, RuntimeError> {
    validate_task_id(task_id)?;
    validate_artifact_id(artifact_id)?;
    Ok(tasks_root
        .join(task_id)
        .join(ARTIFACTS_DIR)
        .join(format!("{artifact_id}.journal")))
}

fn committed_change(
    artifact_id: &str,
    artifact_sequence: u64,
    operations: &[ArtifactOperation],
) -> CommittedArtifactChange {
    let terminal_appends = operations
        .iter()
        .filter_map(|operation| match operation {
            ArtifactOperation::ReplaceDetails { .. } => None,
            ArtifactOperation::AppendTerminal { terminal_id, data } => Some(TerminalOutputAppend {
                terminal_id: terminal_id.clone(),
                data: data.clone(),
            }),
        })
        .collect();
    CommittedArtifactChange {
        artifact_id: artifact_id.to_string(),
        artifact_sequence,
        terminal_appends,
    }
}

fn artifact_io_error(stage: &'static str, error: io::Error) -> RuntimeError {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::InvalidInput => "invalid_input",
        io::ErrorKind::InvalidData => "invalid_data",
        io::ErrorKind::TimedOut => "timed_out",
        io::ErrorKind::UnexpectedEof => "unexpected_eof",
        _ => "other",
    };
    RuntimeError::Storage(format!("Tool artifact {stage} failed (kind={kind})"))
}

fn artifact_frame_error(
    stage: &'static str,
    task_id: &str,
    artifact_id: &str,
    error: RuntimeError,
) -> RuntimeError {
    RuntimeError::Storage(format!(
        "Tool artifact {artifact_id} for Task {task_id} {stage} failed (kind={}, diagnostic={error})",
        error.reason()
    ))
}

#[cfg(test)]
#[path = "artifact_tests.rs"]
mod tests;
