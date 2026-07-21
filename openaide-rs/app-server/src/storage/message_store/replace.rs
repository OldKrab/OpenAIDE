use std::collections::HashMap;

use uuid::Uuid;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ChatMessage;
use crate::protocol::model::NormalizedMessage;
use crate::storage::cursor;
use crate::storage::records::StoredMessage;
use crate::storage::Store;

impl Store {
    pub(crate) fn replace_messages_with_normalized(
        &self,
        task_id: &str,
        messages: Vec<NormalizedMessage>,
    ) -> Result<(), RuntimeError> {
        self.replace_messages_with_normalized_at(task_id, messages, 0)
    }

    /// Replaces Chat from one Native Session load and records the Native Session clock.
    pub(crate) fn replace_messages_with_normalized_at(
        &self,
        task_id: &str,
        messages: Vec<NormalizedMessage>,
        native_updated_at: u128,
    ) -> Result<(), RuntimeError> {
        let projection = self.task_journal().load(task_id)?;
        let existing_ids = projection
            .messages
            .iter()
            .cloned()
            .map(|message| (message.chat.identity, message.chat.message_id))
            .collect::<HashMap<_, _>>();
        let mut stored_messages = Vec::with_capacity(messages.len());
        for (index, mut message) in messages.into_iter().enumerate() {
            self.persist_tool_artifacts(task_id, &mut message)?;
            let sequence = index as u64 + 1;
            stored_messages.push(StoredMessage {
                sequence,
                chat: ChatMessage {
                    cursor: cursor::from_sequence(sequence),
                    identity: message.identity(),
                    message_type: message.message_type().to_string(),
                    message_id: existing_ids
                        .get(&message.identity())
                        .cloned()
                        .unwrap_or_else(|| Uuid::new_v4().to_string()),
                    message,
                },
            });
        }
        self.replace_projection_messages(projection, stored_messages, native_updated_at)
    }
}
