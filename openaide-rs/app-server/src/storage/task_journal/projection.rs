use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, NormalizedMessage};
use crate::storage::id::validate_task_id;
use crate::storage::records::{MessageMeta, StoredMessage};

use super::artifact::validate_artifact_id;
use super::frame::{self, ReplayedFrames};
use super::model::{JournalFrame, TaskOperation, TaskProjection};
use super::store::{RecoveredTask, JOURNAL_FILE, QUARANTINE_FILE};

pub(super) fn validate_operations(
    current: Option<&RecoveredTask>,
    task_id: &str,
    operations: &[TaskOperation],
) -> Result<(), RuntimeError> {
    // Validate against a private shadow so operations in one atomic frame can
    // depend on earlier operations without exposing partially applied state.
    let mut projection = match current {
        Some(RecoveredTask::Available { projection, .. }) => Some(projection.as_ref().clone()),
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
                projection = Some(created.as_ref().clone());
            }
            TaskOperation::ReplaceTask { task } => {
                let projection = projection
                    .as_mut()
                    .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
                if task.task_id != task_id {
                    return Err(RuntimeError::Storage(
                        "Task journal replacement identity does not match".to_string(),
                    ));
                }
                projection.task = task.as_ref().clone();
            }
            TaskOperation::ReplaceProjection {
                projection: replacement,
            } => {
                if projection.is_none() {
                    return Err(RuntimeError::TaskNotFound(task_id.to_string()));
                }
                if replacement.task.task_id != task_id
                    || replacement.message_meta.task_id != task_id
                {
                    return Err(RuntimeError::Storage(
                        "Task journal replacement identities do not match".to_string(),
                    ));
                }
                projection = Some(replacement.as_ref().clone());
            }
            TaskOperation::AppendText { identity, .. } => {
                let projection = projection
                    .as_ref()
                    .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
                validate_text_target(&projection.messages, identity)?;
            }
            TaskOperation::AppendMessage { message } => {
                let projection = projection
                    .as_mut()
                    .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
                validate_message_append(&projection.messages, message)?;
                projection.messages.push(message.as_ref().clone());
            }
            TaskOperation::UpsertMessage { message } => {
                let projection = projection
                    .as_mut()
                    .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
                upsert_message(&mut projection.messages, message.as_ref().clone())?;
            }
            TaskOperation::ReplaceMessages {
                messages,
                message_meta,
            } => {
                let projection = projection
                    .as_mut()
                    .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
                validate_message_meta(task_id, message_meta)?;
                validate_message_set(messages)?;
                projection.messages.clone_from(messages);
                projection.message_meta = message_meta.as_ref().clone();
            }
            TaskOperation::ReplaceMessageMeta { message_meta } => {
                let projection = projection
                    .as_mut()
                    .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
                validate_message_meta(task_id, message_meta)?;
                projection.message_meta = message_meta.as_ref().clone();
            }
            TaskOperation::CommitArtifact {
                artifact_id,
                artifact_sequence,
            } => {
                let projection = projection
                    .as_mut()
                    .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
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
                projection
                    .artifact_heads
                    .insert(artifact_id.clone(), *artifact_sequence);
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
    let mut message_meta_replaced = false;
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
            TaskOperation::ReplaceProjection { projection } => {
                let RecoveredTask::Available {
                    projection: current,
                    ..
                } = state
                    .get_mut(task_id)
                    .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?
                else {
                    return Err(RuntimeError::Storage("Task is unavailable".to_string()));
                };
                *current = projection;
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
            TaskOperation::AppendMessage { message } => {
                let projection = available_projection_mut(state, task_id)?;
                validate_message_append(&projection.messages, &message)?;
                projection.messages.push(*message);
                history_changed = true;
            }
            TaskOperation::UpsertMessage { message } => {
                upsert_message(
                    &mut available_projection_mut(state, task_id)?.messages,
                    *message,
                )?;
                history_changed = true;
            }
            TaskOperation::ReplaceMessages {
                messages,
                message_meta,
            } => {
                validate_message_set(&messages)?;
                validate_message_meta(task_id, &message_meta)?;
                let projection = available_projection_mut(state, task_id)?;
                projection.messages = messages;
                projection.message_meta = *message_meta;
                projection.task.message_history_version = projection.message_meta.version;
                message_meta_replaced = true;
            }
            TaskOperation::ReplaceMessageMeta { message_meta } => {
                validate_message_meta(task_id, &message_meta)?;
                let projection = available_projection_mut(state, task_id)?;
                projection.message_meta = *message_meta;
                projection.task.message_history_version = projection.message_meta.version;
                message_meta_replaced = true;
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
    if history_changed && !message_meta_replaced {
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
        if entry.path().join(QUARANTINE_FILE).exists() {
            tasks.insert(
                task_id,
                RecoveredTask::Unavailable {
                    error: "Task storage is quarantined after a durability failure".to_string(),
                },
            );
            continue;
        }
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
            let result =
                validate_operations(replay_state.get(&task_id), &task_id, &frame.operations)
                    .and_then(|_| {
                        apply_operations(
                            &mut replay_state,
                            &task_id,
                            frame.operations,
                            frame.sequence,
                        )
                    });
            if let Err(error) = result {
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

fn validate_text_target(messages: &[StoredMessage], identity: &str) -> Result<(), RuntimeError> {
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
    messages: &mut [StoredMessage],
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

fn validate_message_meta(task_id: &str, message_meta: &MessageMeta) -> Result<(), RuntimeError> {
    if message_meta.task_id != task_id {
        return Err(RuntimeError::Storage(
            "Task journal message metadata identity does not match".to_string(),
        ));
    }
    Ok(())
}

fn validate_message_set(messages: &[StoredMessage]) -> Result<(), RuntimeError> {
    let mut accepted = Vec::with_capacity(messages.len());
    for message in messages {
        validate_message_append(&accepted, message)?;
        accepted.push(message.clone());
    }
    Ok(())
}

fn validate_message_append(
    messages: &[StoredMessage],
    message: &StoredMessage,
) -> Result<(), RuntimeError> {
    if messages.iter().any(|stored| {
        stored.sequence == message.sequence
            || stored.chat.identity == message.chat.identity
            || stored.chat.message_id == message.chat.message_id
    }) {
        return Err(RuntimeError::Conflict(
            "Task journal message append duplicates durable identity".to_string(),
        ));
    }
    Ok(())
}

fn upsert_message(
    messages: &mut Vec<StoredMessage>,
    message: StoredMessage,
) -> Result<(), RuntimeError> {
    if let Some(index) = messages
        .iter()
        .position(|stored| stored.chat.identity == message.chat.identity)
    {
        if messages.iter().enumerate().any(|(candidate, stored)| {
            candidate != index
                && (stored.sequence == message.sequence
                    || stored.chat.message_id == message.chat.message_id)
        }) {
            return Err(RuntimeError::Conflict(
                "Task journal message upsert collides with durable identity".to_string(),
            ));
        }
        messages[index] = message;
        return Ok(());
    }
    validate_message_append(messages, &message)?;
    messages.push(message);
    Ok(())
}
