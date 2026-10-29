//! Live Chat changes a committed batch publishes alongside its durable frame.

use crate::protocol::errors::RuntimeError;

use super::super::model::{CommittedChatTextAppend, TaskOperation, TaskProjection};

/// Streamed Agent text the batch committed, keyed by the message it extends, so
/// subscribers receive the appended text instead of the whole message.
pub(super) fn committed_chat_text_appends(
    operations: &[TaskOperation],
    projection: &TaskProjection,
) -> Result<Vec<CommittedChatTextAppend>, RuntimeError> {
    operations
        .iter()
        .filter_map(|operation| {
            let TaskOperation::AppendText {
                identity,
                text,
                task_updated_at: Some(_),
                ..
            } = operation
            else {
                return None;
            };
            Some((identity, text))
        })
        .map(|(identity, text)| {
            let message_id = projection
                .messages
                .iter()
                .find(|stored| stored.chat.identity == *identity)
                .map(|stored| stored.chat.message_id.clone())
                .ok_or_else(|| {
                    RuntimeError::Internal(
                        "Committed streamed Chat change is missing from its projection".to_string(),
                    )
                })?;
            Ok(CommittedChatTextAppend {
                message_id,
                text: text.clone(),
            })
        })
        .collect()
}
