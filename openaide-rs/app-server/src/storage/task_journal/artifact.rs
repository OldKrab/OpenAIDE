use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;

use super::frame::{self, ReplayedFrames};
use super::model::{
    ArtifactFrame, ArtifactOperation, CommittedArtifactChange, TerminalOutputAppend,
    ToolArtifactProjection,
};

const ARTIFACTS_DIR: &str = "artifacts";

/// Writes and syncs an artifact frame before its visibility head is referenced
/// by the Task journal. A crash can therefore leave only an invisible orphan.
pub(super) fn prepare(
    tasks_root: &Path,
    task_id: &str,
    artifact_id: &str,
    committed_head: u64,
    operations: Vec<ArtifactOperation>,
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
    if sequence == 1 {
        frame::create(&path, &artifact_frame)?;
    } else {
        frame::append(&path, &artifact_frame)?;
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
    let replayed: ReplayedFrames<ArtifactFrame> = frame::replay(&path)?;
    if replayed.frames.len() < committed_head as usize {
        return Err(RuntimeError::Storage(format!(
            "Tool artifact {artifact_id} ends before committed sequence {committed_head}"
        )));
    }
    let mut terminal_outputs = HashMap::<String, String>::new();
    for artifact_frame in replayed.frames.into_iter().take(committed_head as usize) {
        for operation in artifact_frame.operations {
            match operation {
                ArtifactOperation::AppendTerminal { terminal_id, data } => {
                    terminal_outputs
                        .entry(terminal_id)
                        .or_default()
                        .push_str(&data);
                }
            }
        }
    }
    Ok(ToolArtifactProjection {
        artifact_id: artifact_id.to_string(),
        terminal_outputs,
    })
}

/// Reconciles prepared artifact tails against Task visibility heads at open.
/// A corrupt or missing artifact is isolated to Tool detail and logged; the
/// containing Task projection remains readable.
pub(super) fn reconcile(
    tasks_root: &Path,
    tasks: &HashMap<String, super::store::RecoveredTask>,
) -> Result<(), RuntimeError> {
    for entry in fs::read_dir(tasks_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let task_id = entry.file_name().to_string_lossy().to_string();
        let artifacts_dir = entry.path().join(ARTIFACTS_DIR);
        let Ok(entries) = fs::read_dir(&artifacts_dir) else {
            continue;
        };
        for artifact_entry in entries {
            let artifact_entry = artifact_entry?;
            if !artifact_entry.file_type()?.is_file() {
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
            let committed_head = tasks
                .get(&task_id)
                .and_then(|task| match task {
                    super::store::RecoveredTask::Available { projection, .. } => {
                        projection.artifact_heads.get(artifact_id).copied()
                    }
                    super::store::RecoveredTask::Unavailable { .. } => None,
                })
                .unwrap_or_default();
            let replayed: ReplayedFrames<ArtifactFrame> = match frame::replay(&path) {
                Ok(replayed) => replayed,
                Err(error) => {
                    crate::logging::warn(
                        "tool_artifact_unavailable",
                        serde_json::json!({
                            "task_id": task_id,
                            "artifact_id": artifact_id,
                            "error": error.to_string(),
                        }),
                    );
                    continue;
                }
            };
            if replayed.frames.len() < committed_head as usize {
                crate::logging::warn(
                    "tool_artifact_missing_committed_frame",
                    serde_json::json!({
                        "task_id": task_id,
                        "artifact_id": artifact_id,
                        "committed_head": committed_head,
                        "physical_head": replayed.frames.len(),
                    }),
                );
                continue;
            }
            frame::truncate_after(&path, &replayed, committed_head as usize)?;
        }
    }
    Ok(())
}

pub(super) fn validate_artifact_id(artifact_id: &str) -> Result<(), RuntimeError> {
    if artifact_id.is_empty()
        || !artifact_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(RuntimeError::Storage(format!(
            "Invalid Tool artifact id: {artifact_id}"
        )));
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
        .map(|operation| match operation {
            ArtifactOperation::AppendTerminal { terminal_id, data } => TerminalOutputAppend {
                terminal_id: terminal_id.clone(),
                data: data.clone(),
            },
        })
        .collect();
    CommittedArtifactChange {
        artifact_id: artifact_id.to_string(),
        artifact_sequence,
        terminal_appends,
    }
}
