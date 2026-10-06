//! App Server-clock timing for Chat rows.
//!
//! Timing is recorded only for moments this process witnessed: a row first seen
//! already settled, or rebuilt from a native session, never gains a start, so
//! Chat shows no duration instead of a misleading one.

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, AgentMessageRole, ChatMessage, ChatTiming, NormalizedMessage,
    ObservedSpan,
};
use crate::storage::records::StoredMessage;
use crate::task_events::CommittedChatChange;

use super::TaskMutationContext;

/// Timing for a row entering Chat: a user message records when it was accepted,
/// and work that is still in progress records its start.
pub(super) fn first_observation(message: &NormalizedMessage) -> ChatTiming {
    let observed_at = || message.created_at().to_string();
    ChatTiming {
        sent_at: matches!(message, NormalizedMessage::User { .. }).then(observed_at),
        run: message.is_in_progress().then(|| ObservedSpan {
            started_at: observed_at(),
            ended_at: None,
        }),
        closed_turn: None,
    }
}

/// Timing for a row whose message an Agent update is replacing. The start is
/// never invented here: a row without one stays untimed even if it reports
/// running again.
pub(super) fn carried_over(
    existing: &ChatTiming,
    replacement: &NormalizedMessage,
    now: &str,
) -> ChatTiming {
    let mut timing = existing.clone();
    if let Some(run) = &mut timing.run {
        if replacement.is_in_progress() {
            run.ended_at = None;
        } else if run.ended_at.is_none() {
            run.ended_at = Some(now.to_string());
        }
    }
    timing
}

/// Settles a running Activity and its running steps, closing its observed run.
pub(super) fn finish_running_activity(
    chat: &mut ChatMessage,
    status: ActivityStatus,
    now: &str,
) -> bool {
    let NormalizedMessage::Activity {
        status: activity_status,
        steps,
        ..
    } = &mut chat.message
    else {
        return false;
    };
    if *activity_status != ActivityStatus::Running {
        return false;
    }
    *activity_status = status;
    for step in steps {
        match step {
            ActivityStep::Tool {
                status: step_status,
                ..
            }
            | ActivityStep::Command {
                status: step_status,
                ..
            }
            | ActivityStep::Subagent {
                status: step_status,
                ..
            } if *step_status == ActivityStatus::Running => *step_status = status,
            _ => {}
        }
    }
    if let Some(run) = &mut chat.timing.run {
        run.ended_at.get_or_insert_with(|| now.to_string());
    }
    true
}

impl TaskMutationContext<'_> {
    /// Records the active turn's duration on the Agent answer that closed it.
    /// Call before the turn fields are cleared. A turn that produced no answer
    /// records nothing: there is no row a reader would attribute the time to.
    pub(crate) fn close_turn_on_final_answer(
        &mut self,
        ended_at: &str,
    ) -> Result<(), RuntimeError> {
        let Some(span) = self.active_turn_span(ended_at) else {
            return Ok(());
        };
        let final_answer = self
            .projection
            .messages
            .iter_mut()
            .rev()
            .take_while(|stored| !matches!(stored.chat.message, NormalizedMessage::User { .. }))
            .find(|stored| {
                matches!(
                    stored.chat.message,
                    NormalizedMessage::AgentMessage {
                        role: AgentMessageRole::Agent,
                        ..
                    }
                )
            });
        let changed = final_answer.map(|stored| close_turn(stored, span));
        self.publish_closed_turn("final_answer", changed);
        Ok(())
    }

    /// Records the active turn's duration on the row just appended to end it,
    /// such as an interruption. Call before the turn fields are cleared.
    pub(crate) fn close_turn_on_last_row(&mut self, ended_at: &str) -> Result<(), RuntimeError> {
        let Some(span) = self.active_turn_span(ended_at) else {
            return Ok(());
        };
        let last = self.projection.messages.last_mut();
        let changed = last.map(|stored| close_turn(stored, span));
        self.publish_closed_turn("last_row", changed);
        Ok(())
    }

    fn active_turn_span(&self, ended_at: &str) -> Option<ObservedSpan> {
        Some(ObservedSpan {
            started_at: self.task().active_turn_started_at.clone()?,
            ended_at: Some(ended_at.to_string()),
        })
    }

    /// Logs every close, including one that found no row: that explains a turn
    /// whose duration Chat does not show.
    fn publish_closed_turn(&mut self, target: &'static str, changed: Option<StoredMessage>) {
        let duration_ms = changed.as_ref().and_then(|stored| {
            let span = stored.chat.timing.closed_turn.as_ref()?;
            let started = crate::time::activity_millis(&span.started_at)?;
            let ended = crate::time::activity_millis(span.ended_at.as_deref()?)?;
            Some(ended.saturating_sub(started))
        });
        crate::logging::info(
            "task_turn_duration_recorded",
            serde_json::json!({
                "task_id": self.task().task_id,
                "target": target,
                "recorded": changed.is_some(),
                "duration_ms": duration_ms.map(|duration| duration.to_string()),
            }),
        );
        let Some(stored) = changed else {
            return;
        };
        crate::storage::message_store::advance_message_meta(self.projection, 0);
        self.chat_changes.push(CommittedChatChange::Upsert {
            item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
        });
    }
}

fn close_turn(stored: &mut StoredMessage, span: ObservedSpan) -> StoredMessage {
    stored.chat.timing.closed_turn = Some(span);
    stored.clone()
}

#[cfg(test)]
#[path = "observed_timing_tests.rs"]
mod tests;
