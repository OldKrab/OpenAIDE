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
                    message_id: Uuid::new_v4().to_string(),
                    message,
                },
            });
        }
        self.write_messages(task_id, &stored_messages)?;
        self.write_meta(task_id, &stored_messages)
    }
}
