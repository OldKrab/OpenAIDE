use uuid::Uuid;

#[cfg(test)]
use crate::protocol::errors::RuntimeError;
#[cfg(test)]
use crate::protocol::model::ChatMessage;
use crate::protocol::model::{ActivityStatus, ActivityStep, NormalizedMessage};
#[cfg(test)]
use crate::storage::cursor;
use crate::storage::records::StoredMessage;
use crate::storage::tool_artifacts::PersistedToolDetail;
#[cfg(test)]
use crate::storage::Store;

pub(crate) fn running_turn_message(created_at: &str) -> NormalizedMessage {
    NormalizedMessage::Activity {
        id: Uuid::new_v4().to_string(),
        title: "Working".to_string(),
        status: ActivityStatus::Running,
        created_at: created_at.to_string(),
        collapsed: true,
        steps: vec![ActivityStep::Text {
            text: "Started".to_string(),
            level: Some("info".to_string()),
        }],
    }
}

#[cfg(test)]
pub(crate) fn append_normalized_to_store(
    store: &Store,
    task_id: &str,
    mut message: NormalizedMessage,
) -> Result<StoredMessage, RuntimeError> {
    store.persist_tool_artifacts(task_id, &mut message)?;
    let next_sequence = store
        .read_messages(task_id)?
        .last()
        .map(|m| m.sequence + 1)
        .unwrap_or(1);
    let cursor = cursor::from_sequence(next_sequence);
    let identity = message.identity();
    let chat = ChatMessage {
        cursor,
        message_id: identity.clone(),
        identity,
        message_type: message.message_type().to_string(),
        message,
    };
    store.append_message(task_id, chat)
}

pub(crate) struct UpsertedMessage {
    pub stored: StoredMessage,
    pub tool_details: Vec<PersistedToolDetail>,
}
