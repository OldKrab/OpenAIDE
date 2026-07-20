use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, NormalizedMessage};
use crate::storage::id::validate_task_id;

use super::artifact::validate_artifact_id;
use super::frame::{self, ReplayedFrames};
use super::model::{JournalFrame, TaskOperation, TaskProjection};
use super::store::{RecoveredTask, JOURNAL_FILE};

pub(super) fn validate_operations(
    current: Option<&RecoveredTask>,
    task_id: &str,
    operations: &[TaskOperation],
) -> Result<(), RuntimeError> {
    let mut projection = match current {
        Some(RecoveredTask::Available { projection, .. }) => Some(projection),
        Some(RecoveredTask::Unavailable { error }) => {
            return Err(RuntimeError::Storage(error.clone()))
        }
        None => None,
    };
    for operation in operations {
        match operation {
            TaskOperation::Create {
                projection: created,
            } => {
                if projection.is_some() {
                    return Err(RuntimeError::Conflict(format!(
                        "Task already exists: {task_id}"
                    )));
                }
                if created.task.task_id != task_id || created.message_meta.task_id != task_id {
                    return Err(RuntimeError::Storage(
                        "Task journal create identities do not match".to_string(),
                    ));
                }
                projection = Some(created);
            }
            TaskOperation::ReplaceTask { task } => {
                if projection.is_none() {
                    return Err(RuntimeError::TaskNotFound(task_id.to_string()));
                }
                if task.task_id != task_id {
                    return Err(RuntimeError::Storage(
                        "Task journal replacement identity does not match".to_string(),
                    ));
                }
            }
            TaskOperation::AppendText { identity, .. } => {
                let projection =
                    projection.ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
                validate_text_target(&projection.messages, identity)?;
            }
            TaskOperation::CommitArtifact {
                artifact_id,
                artifact_sequence,
            } => {
                let projection =
                    projection.ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
                validate_artifact_id(artifact_id)?;
                let expected = projection
                    .artifact_heads
                    .get(artifact_id)
                    .copied()
                    .unwrap_or_default()
                    .checked_add(1)
                    .ok_or_else(|| {
                        RuntimeError::Storage("Tool artifact sequence overflow".to_string())
                    })?;
                if *artifact_sequence != expected {
                    return Err(RuntimeError::Storage(format!(
                        "Tool artifact sequence gap: expected {expected}, found {artifact_sequence}"
                    )));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn apply_operations(
    state: &mut HashMap<String, RecoveredTask>,
    task_id: &str,
    operations: Vec<TaskOperation>,
    sequence: u64,
) -> Result<(), RuntimeError> {
    let mut history_changed = false;
    for operation in operations {
        match operation {
            TaskOperation::Create { projection } => {
                state.insert(
                    task_id.to_string(),
                    RecoveredTask::Available {
                        projection,
                        journal_sequence: sequence,
                    },
                );
            }
            TaskOperation::ReplaceTask { task } => {
                available_projection_mut(state, task_id)?.task = *task;
            }
            TaskOperation::AppendText {
                identity,
                text,
                local_history_updated_at,
            } => {
                let projection = available_projection_mut(state, task_id)?;
                append_text(&mut projection.messages, &identity, &text)?;
                projection.message_meta.local_history_updated_at = local_history_updated_at;
                history_changed = true;
            }
            TaskOperation::CommitArtifact {
                artifact_id,
                artifact_sequence,
            } => {
                available_projection_mut(state, task_id)?
                    .artifact_heads
                    .insert(artifact_id, artifact_sequence);
            }
        }
    }
    if history_changed {
        let projection = available_projection_mut(state, task_id)?;
        projection.message_meta.version = projection
            .message_meta
            .version
            .checked_add(1)
            .ok_or_else(|| RuntimeError::Storage("Message history version overflow".to_string()))?;
        projection.task.message_history_version = projection.message_meta.version;
    }
    let RecoveredTask::Available {
        journal_sequence, ..
    } = state
        .get_mut(task_id)
        .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?
    else {
        return Err(RuntimeError::Storage(format!(
            "Task is unavailable: {task_id}"
        )));
    };
    *journal_sequence = sequence;
    Ok(())
}

pub(super) fn replay_tasks(
    tasks_root: &Path,
) -> Result<HashMap<String, RecoveredTask>, RuntimeError> {
    let mut tasks = HashMap::new();
    for entry in fs::read_dir(tasks_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let task_id = entry.file_name().to_string_lossy().to_string();
        validate_task_id(&task_id)?;
        let journal = entry.path().join(JOURNAL_FILE);
        if !journal.exists() {
            continue;
        }
        let replayed: ReplayedFrames<JournalFrame> = match frame::replay(&journal) {
            Ok(replayed) => replayed,
            Err(error) => {
                let message = error.to_string();
                crate::logging::warn(
                    "task_journal_unavailable",
                    serde_json::json!({ "task_id": task_id, "error": message }),
                );
                tasks.insert(task_id, RecoveredTask::Unavailable { error: message });
                continue;
            }
        };
        let mut replay_state = HashMap::new();
        let mut replay_error = None;
        for frame in replayed.frames {
            if let Err(error) = apply_operations(
                &mut replay_state,
                &task_id,
                frame.operations,
                frame.sequence,
            ) {
                replay_error = Some(error.to_string());
                break;
            }
        }
        match replay_error {
            Some(error) => {
                tasks.insert(task_id, RecoveredTask::Unavailable { error });
            }
            None => {
                if let Some(task) = replay_state.remove(&task_id) {
                    tasks.insert(task_id, task);
                }
            }
        }
    }
    Ok(tasks)
}

fn available_projection_mut<'a>(
    state: &'a mut HashMap<String, RecoveredTask>,
    task_id: &str,
) -> Result<&'a mut TaskProjection, RuntimeError> {
    match state.get_mut(task_id) {
        Some(RecoveredTask::Available { projection, .. }) => Ok(projection),
        Some(RecoveredTask::Unavailable { error }) => Err(RuntimeError::Storage(error.clone())),
        None => Err(RuntimeError::TaskNotFound(task_id.to_string())),
    }
}

fn validate_text_target(
    messages: &[crate::storage::records::StoredMessage],
    identity: &str,
) -> Result<(), RuntimeError> {
    let stored = messages
        .iter()
        .find(|stored| stored.chat.identity == identity)
        .ok_or_else(|| RuntimeError::Conflict("Agent text target is missing".to_string()))?;
    let NormalizedMessage::AgentMessage { parts, .. } = &stored.chat.message else {
        return Err(RuntimeError::Conflict(
            "Agent text target changed content channel".to_string(),
        ));
    };
    if !matches!(parts.last(), Some(AgentMessagePart::Text { .. })) {
        return Err(RuntimeError::Conflict(
            "Agent text target has no trailing text part".to_string(),
        ));
    }
    Ok(())
}

fn append_text(
    messages: &mut [crate::storage::records::StoredMessage],
    identity: &str,
    chunk: &str,
) -> Result<(), RuntimeError> {
    validate_text_target(messages, identity)?;
    let stored = messages
        .iter_mut()
        .find(|stored| stored.chat.identity == identity)
        .expect("text target validated above");
    let NormalizedMessage::AgentMessage { parts, .. } = &mut stored.chat.message else {
        unreachable!("text target validated above")
    };
    let Some(AgentMessagePart::Text { text }) = parts.last_mut() else {
        unreachable!("text target validated above")
    };
    text.push_str(chunk);
    Ok(())
}
