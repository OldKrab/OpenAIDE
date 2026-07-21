use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, ChatMessage, NormalizedMessage};
use crate::storage::cursor;
use crate::storage::records::StoredMessage;
use crate::storage::Store;

use super::advance_message_meta;

#[allow(dead_code)]
pub(crate) enum AgentMessageAppend {
    Appended(StoredMessage),
    TextAppended { message_id: String },
    PartAppended(StoredMessage),
}

#[cfg(test)]
impl Store {
    /// Persists one ordered ACP content part using the Agent message identity as correlation.
    pub(crate) fn append_agent_message_part(
        &self,
        task_id: &str,
        message: NormalizedMessage,
    ) -> Result<AgentMessageAppend, RuntimeError> {
        let identity = message.identity();
        let mut projection = self.task_journal().load(task_id)?;
        let result = if let Some(stored) = projection
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == identity)
        {
            let text_appended = append_agent_part(&mut stored.chat.message, message)?;
            let updated = stored.clone();
            if text_appended {
                AgentMessageAppend::TextAppended {
                    message_id: updated.chat.message_id,
                }
            } else {
                AgentMessageAppend::PartAppended(updated)
            }
        } else {
            let sequence = projection
                .messages
                .last()
                .map(|item| item.sequence + 1)
                .unwrap_or(1);
            let stored = StoredMessage {
                sequence,
                chat: ChatMessage {
                    cursor: cursor::from_sequence(sequence),
                    identity: identity.clone(),
                    message_type: message.message_type().to_string(),
                    message_id: identity,
                    message,
                },
            };
            projection.messages.push(stored.clone());
            AgentMessageAppend::Appended(stored)
        };
        advance_message_meta(&mut projection, 0);
        self.commit_task_projection(projection)?;
        Ok(result)
    }
}

fn append_agent_part(
    existing: &mut NormalizedMessage,
    incoming: NormalizedMessage,
) -> Result<bool, RuntimeError> {
    match (existing, incoming) {
        (
            NormalizedMessage::AgentMessage { role, parts, .. },
            NormalizedMessage::AgentMessage {
                role: incoming_role,
                parts: incoming_parts,
                ..
            },
        ) if *role == incoming_role && incoming_parts.len() == 1 => {
            let part = incoming_parts.into_iter().next().expect("one checked part");
            if let (Some(AgentMessagePart::Text { text }), AgentMessagePart::Text { text: chunk }) =
                (parts.last_mut(), &part)
            {
                text.push_str(chunk);
                Ok(true)
            } else {
                parts.push(part);
                Ok(false)
            }
        }
        _ => Err(RuntimeError::Conflict(
            "ACP message id changed content channel".to_string(),
        )),
    }
}
