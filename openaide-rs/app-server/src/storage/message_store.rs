use std::fs;
use std::io::{BufRead, Write};
use std::time::Instant;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStep, AgentMessageRole, ChatMessage, MessagePage, NormalizedMessage,
};

use super::atomic;
use super::cursor;
use super::records::{MessageMeta, StoredMessage};
use super::tool_artifacts::{lightweight_detail_summary, should_replace_input_summary};
use super::Store;

mod agent_append;
mod journal;
mod mutations;
mod replace;
mod transaction;

pub(crate) use agent_append::AgentMessageAppend;
pub(super) use agent_append::AgentMessageCache;

const MESSAGE_BASE_RECORD: &str = "message_base";

#[derive(serde::Deserialize, serde::Serialize)]
struct MessageBaseHeader {
    record_type: String,
    journal_sequence: u64,
}

impl Store {
    pub fn append_message(
        &self,
        task_id: &str,
        message: ChatMessage,
    ) -> Result<StoredMessage, RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        let sequence = messages.last().map(|item| item.sequence + 1).unwrap_or(1);
        let stored = StoredMessage {
            sequence,
            chat: message,
        };
        messages.push(stored.clone());
        self.write_messages(task_id, &messages)?;
        self.write_meta(task_id, &messages)?;
        Ok(stored)
    }

    pub fn upsert_message_by_identity(
        &self,
        task_id: &str,
        mut message: ChatMessage,
    ) -> Result<StoredMessage, RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        if let Some(stored) = messages
            .iter_mut()
            .find(|stored| stored.chat.identity == message.identity)
        {
            message
                .message
                .preserve_tool_permission_outcomes_from(&stored.chat.message);
            message
                .message
                .preserve_interrupted_activity_from(&stored.chat.message);
            message.cursor = stored.chat.cursor.clone();
            message.message_id = stored.chat.message_id.clone();
            message
                .message
                .preserve_created_at_from(&stored.chat.message);
            stored.chat = message;
            let updated = stored.clone();
            self.write_messages(task_id, &messages)?;
            self.write_meta(task_id, &messages)?;
            return Ok(updated);
        }

        let sequence = messages.last().map(|item| item.sequence + 1).unwrap_or(1);
        message.cursor = cursor::from_sequence(sequence);
        let stored = StoredMessage {
            sequence,
            chat: message,
        };
        messages.push(stored.clone());
        self.write_messages(task_id, &messages)?;
        self.write_meta(task_id, &messages)?;
        Ok(stored)
    }

    pub fn tail_page(&self, task_id: &str, limit: usize) -> Result<MessagePage, RuntimeError> {
        let limit = limit.clamp(1, 500);
        let messages = self.read_messages(task_id)?;
        let total = messages.len();
        let start = chat_page_start(&messages, total.saturating_sub(limit), total);
        self.page_from_slice(task_id, &messages, start, total)
    }

    pub fn page_before(
        &self,
        task_id: &str,
        before_cursor: &str,
        limit: usize,
    ) -> Result<MessagePage, RuntimeError> {
        let limit = limit.clamp(1, 500);
        let messages = self.read_messages(task_id)?;
        let before_index = page_before_index(&messages, before_cursor)?;
        let start = chat_page_start(&messages, before_index.saturating_sub(limit), before_index);
        self.page_from_slice(task_id, &messages, start, before_index)
    }

    pub fn read_messages(&self, task_id: &str) -> Result<Vec<StoredMessage>, RuntimeError> {
        self.read_messages_with_journal_sequence(task_id)
            .map(|(messages, _)| messages)
    }

    fn read_messages_with_journal_sequence(
        &self,
        task_id: &str,
    ) -> Result<(Vec<StoredMessage>, u64), RuntimeError> {
        #[cfg(test)]
        self.inner
            .message_file_read_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let task_dir = self.task_dir(task_id)?;
        let path = task_dir.join("messages.jsonl");
        let mut messages = Vec::new();
        let mut applied_journal_sequence = 0;
        if path.exists() {
            let text = fs::read_to_string(path)?;
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                let value: serde_json::Value = serde_json::from_str(line)?;
                if value.get("record_type").and_then(|value| value.as_str())
                    == Some(MESSAGE_BASE_RECORD)
                {
                    applied_journal_sequence =
                        serde_json::from_value::<MessageBaseHeader>(value)?.journal_sequence;
                } else {
                    messages.push(serde_json::from_value(value)?);
                }
            }
        }
        let journal_sequence = journal::replay(&task_dir, applied_journal_sequence, &mut messages)?;
        Ok((messages, journal_sequence))
    }

    /// Materializes the append journal after a prompt boundary without delaying chunk commits.
    pub(crate) fn compact_message_journal(&self, task_id: &str) -> Result<(), RuntimeError> {
        let task_dir = self.task_dir(task_id)?;
        let journal_path = journal::path(&task_dir);
        if !journal_path.exists() {
            return Ok(());
        }
        let started = Instant::now();
        let journal_bytes = journal_path
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        let journal_sequence = journal::latest_sequence(&task_dir)?;
        let applied_sequence = read_applied_journal_sequence(&task_dir.join("messages.jsonl"))?;
        let message_count = if applied_sequence >= journal_sequence {
            None
        } else {
            let messages = self.read_messages(task_id)?;
            let message_count = messages.len();
            self.write_messages(task_id, &messages)?;
            Some(message_count)
        };
        journal::remove(&task_dir)?;
        crate::logging::info(
            "message_journal_compacted",
            serde_json::json!({
                "task_id": task_id,
                "message_count": message_count,
                "journal_bytes": journal_bytes,
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        Ok(())
    }

    pub fn message_history_version(&self, task_id: &str) -> Result<u64, RuntimeError> {
        let meta_path = self.task_dir(task_id)?.join("message_meta.json");
        if meta_path.exists() {
            return self.read_message_version(task_id, 0);
        }
        let messages = self.read_messages(task_id)?;
        Ok(messages.len() as u64)
    }

    /// Timestamp of the latest durable Chat write, independent from Task metadata changes.
    pub fn local_history_updated_at(&self, task_id: &str) -> Result<String, RuntimeError> {
        let path = self.task_dir(task_id)?.join("message_meta.json");
        let text = fs::read_to_string(path)?;
        Ok(serde_json::from_str::<MessageMeta>(&text)?.local_history_updated_at)
    }

    pub fn message_history_has_messages(&self, task_id: &str) -> Result<bool, RuntimeError> {
        let path = self.task_dir(task_id)?.join("message_meta.json");
        if path.exists() {
            let text = fs::read_to_string(path)?;
            return serde_json::from_str::<MessageMeta>(&text)
                .map(|meta| meta.message_count > 0)
                .map_err(RuntimeError::from);
        }
        Ok(!self.read_messages(task_id)?.is_empty())
    }

    fn write_messages(
        &self,
        task_id: &str,
        messages: &[StoredMessage],
    ) -> Result<(), RuntimeError> {
        #[cfg(test)]
        self.inner
            .message_file_write_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut bytes = Vec::new();
        let task_dir = self.task_dir(task_id)?;
        let journal_sequence = journal::latest_sequence(&task_dir)?;
        if journal_sequence > 0 {
            serde_json::to_writer(
                &mut bytes,
                &MessageBaseHeader {
                    record_type: MESSAGE_BASE_RECORD.to_string(),
                    journal_sequence,
                },
            )?;
            writeln!(&mut bytes).map_err(|error| RuntimeError::Storage(error.to_string()))?;
        }
        for message in messages {
            serde_json::to_writer(&mut bytes, message)?;
            writeln!(&mut bytes).map_err(|error| RuntimeError::Storage(error.to_string()))?;
        }
        atomic::write_bytes(&task_dir.join("messages.jsonl"), &bytes)?;
        self.invalidate_agent_message_cache(task_id);
        Ok(())
    }

    fn write_meta(&self, task_id: &str, messages: &[StoredMessage]) -> Result<(), RuntimeError> {
        self.write_meta_at_least(task_id, messages, 0)
    }

    /// Advances the Chat clock to cover an authoritative Native Session snapshot.
    fn write_meta_at_least(
        &self,
        task_id: &str,
        messages: &[StoredMessage],
        minimum_updated_at: u128,
    ) -> Result<(), RuntimeError> {
        let version = self.next_message_version(task_id, messages.len() as u64)?;
        let previous = self
            .local_history_updated_at(task_id)
            .ok()
            .and_then(|value| value.parse::<u128>().ok())
            .unwrap_or_default();
        let now = crate::time::now_string()
            .parse::<u128>()
            .unwrap_or_default();
        let local_history_updated_at = now
            .max(previous.saturating_add(1))
            .max(minimum_updated_at)
            .to_string();
        let meta = MessageMeta {
            task_id: task_id.to_string(),
            version,
            message_count: messages.len() as u64,
            local_history_updated_at,
            first_cursor: messages.first().map(|message| message.chat.cursor.clone()),
            last_cursor: messages.last().map(|message| message.chat.cursor.clone()),
        };
        atomic::write_json(&self.task_dir(task_id)?.join("message_meta.json"), &meta)
    }

    fn page_from_slice(
        &self,
        task_id: &str,
        messages: &[StoredMessage],
        start: usize,
        end: usize,
    ) -> Result<MessagePage, RuntimeError> {
        let version = self.read_message_version(task_id, messages.len() as u64)?;
        let items: Vec<ChatMessage> = messages[start..end]
            .iter()
            .map(|message| self.with_lightweight_tool_summaries(task_id, &message.chat))
            .collect();
        Ok(MessagePage {
            task_id: task_id.to_string(),
            start_cursor: items.first().map(|message| message.cursor.clone()),
            end_cursor: items.last().map(|message| message.cursor.clone()),
            has_before: start > 0,
            total_count: messages.len() as u64,
            version,
            items,
        })
    }

    fn with_lightweight_tool_summaries(&self, task_id: &str, message: &ChatMessage) -> ChatMessage {
        let mut message = message.clone();
        let NormalizedMessage::Activity { steps, .. } = &mut message.message else {
            return message;
        };
        for step in steps {
            let ActivityStep::Tool {
                name,
                input_summary,
                detail_artifact_id,
                details,
                ..
            } = step
            else {
                continue;
            };
            if !should_replace_input_summary(name, input_summary.as_deref()) || details.is_some() {
                continue;
            }
            let Some(artifact_id) = detail_artifact_id.as_deref() else {
                continue;
            };
            if let Ok(details) = self.read_tool_artifact(task_id, artifact_id) {
                if let Some(summary) = lightweight_detail_summary(&details) {
                    *input_summary = Some(summary);
                }
            }
        }
        message
    }

    fn next_message_version(&self, task_id: &str, message_count: u64) -> Result<u64, RuntimeError> {
        let current = self.read_message_version(task_id, 0)?;
        Ok(current.saturating_add(1).max(message_count))
    }

    fn read_message_version(&self, task_id: &str, fallback: u64) -> Result<u64, RuntimeError> {
        let path = self.task_dir(task_id)?.join("message_meta.json");
        if !path.exists() {
            return Ok(fallback);
        }
        let text = fs::read_to_string(path)?;
        serde_json::from_str::<MessageMeta>(&text)
            .map(|meta| meta.version)
            .map_err(RuntimeError::from)
    }

    fn invalidate_agent_message_cache(&self, task_id: &str) {
        self.inner
            .agent_message_cache
            .lock()
            .expect("Agent message cache poisoned")
            .remove(task_id);
    }
}

fn read_applied_journal_sequence(path: &std::path::Path) -> Result<u64, RuntimeError> {
    if !path.exists() {
        return Ok(0);
    }
    let mut first_line = String::new();
    std::io::BufReader::new(fs::File::open(path)?).read_line(&mut first_line)?;
    if first_line.trim().is_empty() {
        return Ok(0);
    }
    let value: serde_json::Value = serde_json::from_str(&first_line)?;
    if value.get("record_type").and_then(|value| value.as_str()) != Some(MESSAGE_BASE_RECORD) {
        return Ok(0);
    }
    Ok(serde_json::from_value::<MessageBaseHeader>(value)?.journal_sequence)
}

const TARGET_CHAT_TURNS: usize = 10;
const MAX_SEMANTIC_WINDOW_RECORDS: usize = 500;

fn chat_page_start(messages: &[StoredMessage], requested_start: usize, end: usize) -> usize {
    if requested_start == 0 || requested_start >= messages.len() {
        return requested_start;
    }

    // A Chat page is sized for useful conversation context. The raw record limit remains the
    // ordinary payload budget, while this bounded scan includes up to ten recent user turns.
    let scan_floor = end.saturating_sub(MAX_SEMANTIC_WINDOW_RECORDS);
    let mut turn_start = None;
    let mut user_turns = 0;
    for index in (scan_floor..end).rev() {
        if matches!(
            &messages[index].chat.message,
            NormalizedMessage::User { .. }
        ) {
            turn_start = Some(index);
            user_turns += 1;
            if user_turns == TARGET_CHAT_TURNS {
                break;
            }
        }
    }
    let requested_start = turn_start
        .map(|turn_start| requested_start.min(turn_start))
        .unwrap_or(requested_start);
    if !matches!(
        &messages[requested_start].chat.message,
        NormalizedMessage::Activity { .. }
            | NormalizedMessage::AgentMessage {
                role: AgentMessageRole::Thought,
                ..
            }
    ) {
        return requested_start;
    }

    let mut run_start = requested_start;
    while run_start > 0
        && matches!(
            &messages[run_start].chat.message,
            NormalizedMessage::Activity { .. }
                | NormalizedMessage::AgentMessage {
                    role: AgentMessageRole::Thought,
                    ..
                }
        )
    {
        run_start -= 1;
    }
    if matches!(
        &messages[run_start].chat.message,
        NormalizedMessage::User { .. }
    ) {
        run_start
    } else {
        run_start + 1
    }
}

fn page_before_index(
    messages: &[StoredMessage],
    before_cursor: &str,
) -> Result<usize, RuntimeError> {
    if let Ok(before) = cursor::to_sequence(before_cursor) {
        if let Some(index) = messages
            .iter()
            .position(|message| message.sequence == before)
        {
            return Ok(index);
        }
    }

    messages
        .iter()
        .position(|message| message.chat.cursor == before_cursor)
        .ok_or_else(|| RuntimeError::InvalidParams("before_cursor".to_string()))
}
