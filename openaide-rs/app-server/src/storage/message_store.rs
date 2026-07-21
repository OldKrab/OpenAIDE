use crate::chat_history::ChatHistoryPolicy;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStep, AgentMessageRole, ChatMessage, MessagePage, NormalizedMessage,
};
use crate::storage::records::{MessageMeta, StoredMessage};
use crate::storage::task_journal::{TaskProjection, TaskWrite};

use super::cursor;
use super::tool_artifacts::{lightweight_detail_summary, should_replace_input_summary};
use super::Store;

#[cfg(test)]
mod agent_append;
#[cfg(test)]
mod mutations;
#[cfg(test)]
mod replace;

#[cfg(test)]
pub(crate) use agent_append::AgentMessageAppend;

impl Store {
    #[cfg(test)]
    pub fn append_message(
        &self,
        task_id: &str,
        message: ChatMessage,
    ) -> Result<StoredMessage, RuntimeError> {
        let mut projection = self.task_journal().load(task_id)?;
        let sequence = projection
            .messages
            .last()
            .map(|item| item.sequence + 1)
            .unwrap_or(1);
        let stored = StoredMessage {
            sequence,
            chat: message,
        };
        projection.messages.push(stored.clone());
        advance_message_meta(&mut projection, 0);
        self.commit_task_projection(projection)?;
        Ok(stored)
    }

    #[cfg(test)]
    pub fn upsert_message_by_identity(
        &self,
        task_id: &str,
        mut message: ChatMessage,
    ) -> Result<StoredMessage, RuntimeError> {
        let mut projection = self.task_journal().load(task_id)?;
        let updated = if let Some(stored) = projection
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == message.identity)
        {
            message
                .message
                .preserve_tool_permission_outcomes_from(&stored.chat.message);
            message.cursor = stored.chat.cursor.clone();
            message.message_id = stored.chat.message_id.clone();
            message
                .message
                .preserve_created_at_from(&stored.chat.message);
            stored.chat = message;
            stored.clone()
        } else {
            let sequence = projection
                .messages
                .last()
                .map(|item| item.sequence + 1)
                .unwrap_or(1);
            message.cursor = cursor::from_sequence(sequence);
            let stored = StoredMessage {
                sequence,
                chat: message,
            };
            projection.messages.push(stored.clone());
            stored
        };
        advance_message_meta(&mut projection, 0);
        self.commit_task_projection(projection)?;
        Ok(updated)
    }

    pub fn tail_page(&self, task_id: &str, limit: usize) -> Result<MessagePage, RuntimeError> {
        let limit = limit.clamp(1, 500);
        let projection = self.task_journal().load(task_id)?;
        let total = projection.messages.len();
        let start = chat_page_start(&projection.messages, total.saturating_sub(limit), total);
        self.page_from_projection(&projection, start, total)
    }

    pub fn page_before(
        &self,
        task_id: &str,
        before_cursor: &str,
        limit: usize,
    ) -> Result<MessagePage, RuntimeError> {
        let limit = limit.clamp(1, 500);
        let projection = self.task_journal().load(task_id)?;
        let end = page_before_index(&projection.messages, before_cursor)?;
        let start = chat_page_start(&projection.messages, end.saturating_sub(limit), end);
        self.page_from_projection(&projection, start, end)
    }

    pub fn read_messages(&self, task_id: &str) -> Result<Vec<StoredMessage>, RuntimeError> {
        #[cfg(test)]
        self.inner
            .message_file_read_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.task_journal().load(task_id)?.messages)
    }

    /// Checks the journal's measured reclaim threshold at an idle prompt boundary.
    pub(crate) fn compact_message_journal(&self, task_id: &str) -> Result<(), RuntimeError> {
        self.task_journal()
            .submit(TaskWrite::compaction_if_worthwhile_barrier(
                task_id.to_string(),
            ))?
            .wait()?;
        Ok(())
    }

    pub fn message_history_version(&self, task_id: &str) -> Result<u64, RuntimeError> {
        Ok(self.task_journal().load(task_id)?.message_meta.version)
    }

    pub fn local_history_updated_at(&self, task_id: &str) -> Result<String, RuntimeError> {
        Ok(self
            .task_journal()
            .load(task_id)?
            .message_meta
            .local_history_updated_at)
    }

    pub fn message_history_has_messages(&self, task_id: &str) -> Result<bool, RuntimeError> {
        Ok(!self.task_journal().load(task_id)?.messages.is_empty())
    }

    #[cfg(test)]
    pub(super) fn replace_projection_messages(
        &self,
        mut projection: TaskProjection,
        messages: Vec<StoredMessage>,
        minimum_updated_at: u128,
    ) -> Result<(), RuntimeError> {
        projection.messages = messages;
        advance_message_meta(&mut projection, minimum_updated_at);
        self.commit_task_projection(projection)
    }

    #[cfg(test)]
    pub(super) fn commit_task_projection(
        &self,
        projection: TaskProjection,
    ) -> Result<(), RuntimeError> {
        #[cfg(test)]
        self.inner
            .message_file_write_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.task_journal()
            .submit(TaskWrite::barrier_replace_projection(
                projection,
                Vec::new(),
            ))?
            .wait()?;
        Ok(())
    }

    pub(crate) fn page_from_projection(
        &self,
        projection: &TaskProjection,
        start: usize,
        end: usize,
    ) -> Result<MessagePage, RuntimeError> {
        let items = projection.messages[start..end]
            .iter()
            .map(|message| {
                self.with_lightweight_tool_summaries(&projection.task.task_id, &message.chat)
            })
            .collect::<Vec<_>>();
        Ok(MessagePage {
            task_id: projection.task.task_id.clone(),
            start_cursor: items.first().map(|message| message.cursor.clone()),
            end_cursor: items.last().map(|message| message.cursor.clone()),
            has_before: start > 0,
            total_count: projection.messages.len() as u64,
            version: projection.message_meta.version,
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
}

pub(crate) fn advance_message_meta(projection: &mut TaskProjection, minimum_updated_at: u128) {
    let previous = projection
        .message_meta
        .local_history_updated_at
        .parse::<u128>()
        .unwrap_or_default();
    let now = crate::time::now_string()
        .parse::<u128>()
        .unwrap_or_default();
    projection.message_meta = MessageMeta {
        task_id: projection.task.task_id.clone(),
        version: projection
            .message_meta
            .version
            .saturating_add(1)
            .max(projection.messages.len() as u64),
        message_count: projection.messages.len() as u64,
        local_history_updated_at: now
            .max(previous.saturating_add(1))
            .max(minimum_updated_at)
            .to_string(),
        first_cursor: projection
            .messages
            .first()
            .map(|message| message.chat.cursor.clone()),
        last_cursor: projection
            .messages
            .last()
            .map(|message| message.chat.cursor.clone()),
    };
}

const TARGET_CHAT_TURNS: usize = 10;
const MAX_SEMANTIC_WINDOW_RECORDS: usize = 500;

pub(crate) fn chat_page_start(
    messages: &[StoredMessage],
    requested_start: usize,
    end: usize,
) -> usize {
    if requested_start == 0 || requested_start >= messages.len() {
        return requested_start;
    }
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

#[allow(dead_code)]
fn _default_tail_limit() -> usize {
    ChatHistoryPolicy::default().task_snapshot_tail_limit()
}
