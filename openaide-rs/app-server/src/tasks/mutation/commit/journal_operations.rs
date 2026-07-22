use std::collections::HashSet;

use crate::protocol::errors::RuntimeError;
use crate::storage::task_journal::{TaskOperation, TaskProjection};
use crate::task_events::CommittedChatChange;

/// Converts publication facts into the smallest authoritative storage delta.
/// Publication remains shaped for clients, while persistence retains stable
/// message identities and never serializes unchanged Chat history.
pub(super) fn journal_operations(
    projection: &TaskProjection,
    chat: &[CommittedChatChange],
) -> Result<Vec<TaskOperation>, RuntimeError> {
    let mut operations = vec![TaskOperation::ReplaceTask {
        task: Box::new(projection.task.clone()),
    }];
    if chat.is_empty() {
        return Ok(operations);
    }
    if chat
        .iter()
        .any(|change| matches!(change, CommittedChatChange::Replace))
    {
        operations.push(TaskOperation::ReplaceMessages {
            messages: projection.messages.clone(),
            message_meta: Box::new(projection.message_meta.clone()),
        });
        return Ok(operations);
    }

    let appended = chat
        .iter()
        .filter_map(|change| match change {
            CommittedChatChange::Append { item } => Some(item.message_id.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let upserted = chat
        .iter()
        .filter_map(|change| match change {
            CommittedChatChange::Upsert { item } => Some(item.message_id.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let mut emitted = HashSet::new();
    for change in chat {
        let message_id = match change {
            CommittedChatChange::Append { item } | CommittedChatChange::Upsert { item } => {
                item.message_id.as_str()
            }
            CommittedChatChange::AppendText { message_id, .. } => message_id.as_str(),
            CommittedChatChange::Replace => unreachable!("replacement handled above"),
        };
        let stored = projection
            .messages
            .iter()
            .find(|stored| stored.chat.message_id == message_id)
            .ok_or_else(|| {
                RuntimeError::Internal(
                    "Committed Chat change is missing from its durable projection".to_string(),
                )
            })?;
        if appended.contains(message_id) {
            if emitted.insert(message_id) {
                operations.push(TaskOperation::AppendMessage {
                    message: Box::new(stored.clone()),
                });
            }
        } else if upserted.contains(message_id) {
            if emitted.insert(message_id) {
                operations.push(TaskOperation::UpsertMessage {
                    message: Box::new(stored.clone()),
                });
            }
        } else if let CommittedChatChange::AppendText { text, .. } = change {
            operations.push(TaskOperation::AppendText {
                identity: stored.chat.identity.clone(),
                text: text.clone(),
                local_history_updated_at: projection.message_meta.local_history_updated_at.clone(),
            });
        }
    }
    operations.push(TaskOperation::ReplaceMessageMeta {
        message_meta: Box::new(projection.message_meta.clone()),
    });
    Ok(operations)
}
