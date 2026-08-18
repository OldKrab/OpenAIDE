use std::collections::BTreeMap;

use super::super::model::{ArtifactOperation, TaskOperation};
use super::super::scheduler::QueuedWrite;

pub(super) struct ReducedBatch {
    pub(super) task_operations: Vec<TaskOperation>,
    pub(super) artifacts: BTreeMap<String, Vec<ArtifactOperation>>,
}

pub(super) fn reduce_batch(batch: &[QueuedWrite]) -> ReducedBatch {
    let mut task_operations = Vec::new();
    let mut artifacts = BTreeMap::<String, Vec<ArtifactOperation>>::new();
    for queued in batch {
        for operation in &queued.write.operations {
            match operation {
                TaskOperation::AppendText {
                    identity,
                    text,
                    local_history_updated_at,
                    task_updated_at,
                } => match task_operations.last_mut() {
                    Some(TaskOperation::AppendText {
                        identity: existing_identity,
                        text: existing_text,
                        local_history_updated_at: existing_updated_at,
                        task_updated_at: existing_task_updated_at,
                    }) if existing_identity == identity => {
                        existing_text.push_str(text);
                        existing_updated_at.clone_from(local_history_updated_at);
                        if task_updated_at.is_some() {
                            existing_task_updated_at.clone_from(task_updated_at);
                        }
                    }
                    _ => task_operations.push(TaskOperation::AppendText {
                        identity: identity.clone(),
                        text: text.clone(),
                        local_history_updated_at: local_history_updated_at.clone(),
                        task_updated_at: task_updated_at.clone(),
                    }),
                },
                TaskOperation::Create { projection } => {
                    task_operations.push(TaskOperation::Create {
                        projection: projection.clone(),
                    })
                }
                TaskOperation::ReplaceTask { task } => {
                    task_operations.push(TaskOperation::ReplaceTask { task: task.clone() })
                }
                TaskOperation::ReplaceProjection { projection } => {
                    task_operations.push(TaskOperation::ReplaceProjection {
                        projection: projection.clone(),
                    })
                }
                TaskOperation::AppendMessage { message } => {
                    task_operations.push(TaskOperation::AppendMessage {
                        message: message.clone(),
                    })
                }
                TaskOperation::UpsertMessage { message } => {
                    task_operations.push(TaskOperation::UpsertMessage {
                        message: message.clone(),
                    })
                }
                TaskOperation::ReplaceMessages {
                    messages,
                    message_meta,
                } => task_operations.push(TaskOperation::ReplaceMessages {
                    messages: messages.clone(),
                    message_meta: message_meta.clone(),
                }),
                TaskOperation::ReplaceMessageMeta { message_meta } => {
                    task_operations.push(TaskOperation::ReplaceMessageMeta {
                        message_meta: message_meta.clone(),
                    })
                }
                TaskOperation::CommitArtifact { .. } => {
                    unreachable!("artifact commit references are worker-owned")
                }
            }
        }
        for write in &queued.write.artifacts {
            match &write.operation {
                ArtifactOperation::ReplaceDetails { details } => {
                    artifacts
                        .entry(write.artifact_id.clone())
                        .or_default()
                        .push(ArtifactOperation::ReplaceDetails {
                            details: details.clone(),
                        });
                }
                ArtifactOperation::AppendTerminal { terminal_id, data } => {
                    if data.is_empty() {
                        continue;
                    }
                    let operations = artifacts.entry(write.artifact_id.clone()).or_default();
                    match operations.last_mut() {
                        Some(ArtifactOperation::AppendTerminal {
                            terminal_id: existing_terminal_id,
                            data: existing_data,
                        }) if existing_terminal_id == terminal_id => existing_data.push_str(data),
                        _ => operations.push(write.operation.clone()),
                    }
                }
            }
        }
    }
    ReducedBatch {
        task_operations,
        artifacts,
    }
}
