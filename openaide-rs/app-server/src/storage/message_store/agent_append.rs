use std::collections::HashMap;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, ChatMessage, NormalizedMessage};
use crate::storage::cursor;
use crate::storage::records::StoredMessage;
use crate::storage::Store;

use super::journal;

pub(crate) enum AgentMessageAppend {
    Appended(StoredMessage),
    TextAppended { message_id: String },
    PartAppended(StoredMessage),
}

pub(in crate::storage) struct AgentMessageCache {
    messages: Vec<StoredMessage>,
    positions: HashMap<String, usize>,
    journal_sequence: u64,
}

impl AgentMessageCache {
    fn new(messages: Vec<StoredMessage>, journal_sequence: u64) -> Self {
        let positions = messages
            .iter()
            .enumerate()
            .map(|(index, message)| (message.chat.identity.clone(), index))
            .collect();
        Self {
            messages,
            positions,
            journal_sequence,
        }
    }
}

impl Store {
    /// Persists one ordered ACP content part using the Agent message identity as correlation.
    pub(crate) fn append_agent_message_part(
        &self,
        task_id: &str,
        message: NormalizedMessage,
    ) -> Result<AgentMessageAppend, RuntimeError> {
        if let Some((role, text)) = single_agent_text_chunk(&message) {
            return self.append_agent_text_chunk(
                task_id,
                message.identity(),
                role,
                text.to_string(),
                message,
            );
        }
        self.append_agent_message_part_materialized(task_id, message)
    }

    fn append_agent_message_part_materialized(
        &self,
        task_id: &str,
        message: NormalizedMessage,
    ) -> Result<AgentMessageAppend, RuntimeError> {
        let identity = message.identity();
        let mut messages = self.read_messages(task_id)?;
        if let Some(stored) = messages
            .iter_mut()
            .find(|stored| stored.chat.identity == identity)
        {
            let text_appended = append_agent_part(&mut stored.chat.message, message)?;
            let updated = stored.clone();
            self.write_messages(task_id, &messages)?;
            self.write_meta(task_id, &messages)?;
            return Ok(if text_appended {
                AgentMessageAppend::TextAppended {
                    message_id: updated.chat.message_id,
                }
            } else {
                AgentMessageAppend::PartAppended(updated)
            });
        }

        let sequence = messages.last().map(|item| item.sequence + 1).unwrap_or(1);
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
        messages.push(stored.clone());
        self.write_messages(task_id, &messages)?;
        self.write_meta(task_id, &messages)?;
        Ok(AgentMessageAppend::Appended(stored))
    }

    fn append_agent_text_chunk(
        &self,
        task_id: &str,
        identity: String,
        role: AgentMessageRole,
        text: String,
        message: NormalizedMessage,
    ) -> Result<AgentMessageAppend, RuntimeError> {
        let task_dir = self.task_dir(task_id)?;
        let mut cache = self
            .inner
            .agent_message_cache
            .lock()
            .expect("Agent message cache poisoned");
        if !cache.contains_key(task_id) {
            drop(cache);
            let (messages, journal_sequence) = self.read_messages_with_journal_sequence(task_id)?;
            cache = self
                .inner
                .agent_message_cache
                .lock()
                .expect("Agent message cache poisoned");
            cache
                .entry(task_id.to_string())
                .or_insert_with(|| AgentMessageCache::new(messages, journal_sequence));
        }
        let cached = cache.get_mut(task_id).expect("cache inserted above");
        let Some(index) = cached.positions.get(&identity).copied() else {
            let sequence = cached
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
                    message_id: identity.clone(),
                    message,
                },
            };
            let next_sequence = cached.journal_sequence.saturating_add(1);
            let previous_len = journal::append_message(&task_dir, next_sequence, &stored)?;
            cached
                .positions
                .insert(identity.clone(), cached.messages.len());
            cached.messages.push(stored.clone());
            if let Err(error) = self.write_meta(task_id, &cached.messages) {
                cached.messages.pop();
                cached.positions.remove(&identity);
                journal::truncate(&task_dir, previous_len)?;
                return Err(error);
            }
            cached.journal_sequence = next_sequence;
            return Ok(AgentMessageAppend::Appended(stored));
        };
        let next_sequence = cached.journal_sequence.saturating_add(1);
        let message_id = {
            let stored = &cached.messages[index];
            let NormalizedMessage::AgentMessage {
                role: existing_role,
                parts,
                ..
            } = &stored.chat.message
            else {
                return Err(RuntimeError::Conflict(
                    "ACP message id changed content channel".to_string(),
                ));
            };
            if *existing_role != role {
                return Err(RuntimeError::Conflict(
                    "ACP message id changed content channel".to_string(),
                ));
            }
            if !matches!(parts.last(), Some(AgentMessagePart::Text { .. })) {
                drop(cache);
                return self.append_agent_message_part_materialized(task_id, message);
            }
            stored.chat.message_id.clone()
        };
        let previous_len = journal::append_text(&task_dir, next_sequence, &identity, &text)?;
        append_cached_text(&mut cached.messages[index], &text);
        if let Err(error) = self.write_meta(task_id, &cached.messages) {
            truncate_cached_text(&mut cached.messages[index], text.len());
            journal::truncate(&task_dir, previous_len)?;
            return Err(error);
        }
        cached.journal_sequence = next_sequence;
        Ok(AgentMessageAppend::TextAppended { message_id })
    }
}

fn single_agent_text_chunk(message: &NormalizedMessage) -> Option<(AgentMessageRole, &str)> {
    let NormalizedMessage::AgentMessage { role, parts, .. } = message else {
        return None;
    };
    match parts.as_slice() {
        [AgentMessagePart::Text { text }] => Some((*role, text)),
        _ => None,
    }
}

fn append_cached_text(message: &mut StoredMessage, chunk: &str) {
    let NormalizedMessage::AgentMessage { parts, .. } = &mut message.chat.message else {
        unreachable!("Agent message cache validation precedes append");
    };
    let Some(AgentMessagePart::Text { text }) = parts.last_mut() else {
        unreachable!("Agent message cache validation requires a trailing text part");
    };
    text.push_str(chunk);
}

fn truncate_cached_text(message: &mut StoredMessage, chunk_len: usize) {
    let NormalizedMessage::AgentMessage { parts, .. } = &mut message.chat.message else {
        unreachable!("Agent message cache validation precedes rollback");
    };
    let Some(AgentMessagePart::Text { text }) = parts.last_mut() else {
        unreachable!("Agent message cache validation requires a trailing text part");
    };
    text.truncate(text.len().saturating_sub(chunk_len));
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
