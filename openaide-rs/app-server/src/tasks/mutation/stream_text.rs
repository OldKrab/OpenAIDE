use std::collections::HashMap;
use std::sync::{Condvar, Mutex};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, NormalizedMessage};

use super::TaskMutations;

/// Tracks admitted Agent-text writes until their durable publication has been
/// handed to the Task update channel. Synchronous mutations use this as a
/// narrow ordering barrier; streamed Tool output remains outside the barrier.
#[derive(Default)]
pub(super) struct PendingStreamText {
    counts: Mutex<HashMap<String, usize>>,
    drained: Condvar,
}

impl PendingStreamText {
    pub(super) fn mark(&self, task_id: &str) {
        let mut counts = self.counts.lock().expect("pending text state poisoned");
        *counts.entry(task_id.to_string()).or_default() += 1;
    }

    pub(super) fn complete(&self, task_id: &str, count: usize) {
        if count == 0 {
            return;
        }
        let mut counts = self.counts.lock().expect("pending text state poisoned");
        if let Some(pending) = counts.get_mut(task_id) {
            *pending = pending.saturating_sub(count);
            if *pending == 0 {
                counts.remove(task_id);
            }
        }
        self.drained.notify_all();
    }

    pub(super) fn fail(&self, task_id: &str) {
        self.counts
            .lock()
            .expect("pending text state poisoned")
            .remove(task_id);
        self.drained.notify_all();
    }

    fn has_pending(&self, task_id: &str) -> bool {
        self.counts
            .lock()
            .expect("pending text state poisoned")
            .contains_key(task_id)
    }

    fn wait_until_drained(&self, task_id: &str) {
        let mut counts = self.counts.lock().expect("pending text state poisoned");
        while counts.contains_key(task_id) {
            counts = self
                .drained
                .wait(counts)
                .expect("pending text state poisoned");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentMessageTextStreamOutcome {
    Admitted,
    NeedsMessageCommit,
    IgnoredStaleSession,
}

impl TaskMutations {
    /// Admits a text delta to the bounded stream lane. The first chunk of a
    /// message still uses the ordinary barrier because it creates the Chat
    /// row; later chunks are coalesced and published after durable commit.
    pub(crate) fn stream_agent_message_text(
        &self,
        task_id: &str,
        expected_session_id: &str,
        role: crate::protocol::model::AgentMessageRole,
        identity: &str,
        text: &str,
        updated_at: &str,
    ) -> Result<AgentMessageTextStreamOutcome, RuntimeError> {
        // The journal reducer treats empty AppendText operations as semantic
        // no-ops and would not emit the acknowledgement used by the ordering
        // barrier. Preserve the ordinary mutation path for this edge case.
        if text.is_empty() {
            return Ok(AgentMessageTextStreamOutcome::NeedsMessageCommit);
        }
        let mut write =
            crate::storage::task_journal::TaskWrite::stream_append_text_with_task_update(
                task_id.to_string(),
                identity.to_string(),
                text.to_string(),
                updated_at.to_string(),
                updated_at.to_string(),
            );
        loop {
            let guard = self.lock();
            let projection = self.store.task_journal().load(task_id)?;
            if projection.task.agent_session_id.as_deref() != Some(expected_session_id) {
                return Ok(AgentMessageTextStreamOutcome::IgnoredStaleSession);
            }
            if projection.task.lifecycle.is_archived() {
                return Ok(AgentMessageTextStreamOutcome::IgnoredStaleSession);
            }
            let Some(stored) = projection
                .messages
                .iter()
                .find(|stored| stored.chat.identity == identity)
            else {
                return Ok(AgentMessageTextStreamOutcome::NeedsMessageCommit);
            };
            let streamable = matches!(
                &stored.chat.message,
                NormalizedMessage::AgentMessage {
                    role: stored_role,
                    parts,
                    ..
                } if stored_role == &role
                    && matches!(parts.last(), Some(AgentMessagePart::Text { .. }))
            );
            if !streamable {
                return Ok(AgentMessageTextStreamOutcome::NeedsMessageCommit);
            }
            self.pending_stream_text.mark(task_id);
            match self.store.task_journal().try_submit(write) {
                Err(error) => {
                    self.pending_stream_text.complete(task_id, 1);
                    return Err(error);
                }
                Ok(crate::storage::task_journal::TrySubmit::Admitted(receipt)) => {
                    drop(guard);
                    drop(receipt);
                    return Ok(AgentMessageTextStreamOutcome::Admitted);
                }
                Ok(crate::storage::task_journal::TrySubmit::Full(returned)) => {
                    self.pending_stream_text.complete(task_id, 1);
                    write = returned;
                    drop(guard);
                    self.store.task_journal().wait_for_capacity(&write)?;
                }
            }
        }
    }

    pub(super) fn flush_streamed_agent_text(&self, task_id: &str) -> Result<(), RuntimeError> {
        if !self.pending_stream_text.has_pending(task_id) {
            return Ok(());
        }
        self.store
            .task_journal()
            .submit(crate::storage::task_journal::TaskWrite::barrier_streamed_text(task_id))?
            .wait()?;
        self.pending_stream_text.wait_until_drained(task_id);
        Ok(())
    }
}
