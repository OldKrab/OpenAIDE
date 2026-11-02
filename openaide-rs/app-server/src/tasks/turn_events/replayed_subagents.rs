use std::collections::HashMap;

use crate::agent::events::{
    AgentNativeSubagentState, AgentNativeSubagentStateUpdate, AgentReplayedSubagentEvent,
};
use crate::agent::AgentSessionEventSink;
use crate::protocol::errors::RuntimeError;

use super::TaskSessionEventSink;

/// How the rest of one child's replay applies.
#[derive(Clone)]
enum ReplayedChild {
    /// Rebuilt from this replay; its parent and outcome so far.
    Restored {
        parent_native_session_id: String,
        outcome_known: bool,
    },
    /// The Task already holds this child, and its saved history stays.
    Stored,
    /// A write failed, so its history is not whole. No later event applies.
    Failed,
}

impl TaskSessionEventSink {
    /// Rebuilds the children a committed `session/load` replay reported through
    /// the same path their live traffic takes. Runs after the replaced Chat
    /// commits and before the session's events attach, so Agent reports after the
    /// load follow the children they describe.
    ///
    /// A child's failure stays with that child ([ADR-0076]): a Subagent History
    /// never fails its Task. A child whose replay ends without an outcome is
    /// Disconnected until the Agent reports one.
    ///
    /// [ADR-0076]: ../../../../../docs/adr/0076-subagent-histories-are-task-journal-chat-streams.md
    pub(crate) fn restore_replayed_subagents(&self, events: Vec<AgentReplayedSubagentEvent>) {
        if events.is_empty() {
            return;
        }
        let started = std::time::Instant::now();
        let event_count = events.len();
        crate::logging::info(
            "task_subagent_replay_restore_started",
            serde_json::json!({
                "task_id": self.task_id,
                "event_count": event_count,
            }),
        );
        let mut children = HashMap::<String, ReplayedChild>::new();
        for event in events {
            let child = native_session_id(&event).to_string();
            if let Err(error) = self.apply_replayed_event(event, &mut children) {
                self.log_replayed_child_failure(&error);
                children.insert(child, ReplayedChild::Failed);
            }
        }
        let mut restored_count = 0_usize;
        for (native_session_id, child) in &children {
            let ReplayedChild::Restored {
                parent_native_session_id,
                outcome_known,
            } = child
            else {
                continue;
            };
            restored_count += 1;
            if *outcome_known {
                continue;
            }
            if let Err(error) = self.subagent_state_changed(AgentNativeSubagentStateUpdate {
                parent_native_session_id: parent_native_session_id.clone(),
                native_session_id: native_session_id.clone(),
                state: AgentNativeSubagentState::Disconnected,
            }) {
                self.log_replayed_child_failure(&error);
            }
        }
        let count = |wanted: fn(&ReplayedChild) -> bool| {
            children.values().filter(|child| wanted(child)).count()
        };
        crate::logging::info(
            "task_subagent_replay_restore_finished",
            serde_json::json!({
                "task_id": self.task_id,
                "event_count": event_count,
                "restored_count": restored_count,
                "stored_count": count(|child| matches!(child, ReplayedChild::Stored)),
                "failed_count": count(|child| matches!(child, ReplayedChild::Failed)),
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
    }

    fn apply_replayed_event(
        &self,
        event: AgentReplayedSubagentEvent,
        children: &mut HashMap<String, ReplayedChild>,
    ) -> Result<(), RuntimeError> {
        if let AgentReplayedSubagentEvent::Spawned(spawned) = event {
            if children.contains_key(&spawned.native_session_id) {
                return Ok(());
            }
            let stored = match self
                .mutations
                .store()
                .subagent_record_by_native(&self.task_id, &spawned.native_session_id)
            {
                Ok(_) => true,
                Err(RuntimeError::TaskNotFound(_)) => false,
                Err(error) => return Err(error),
            };
            let child = if stored {
                ReplayedChild::Stored
            } else {
                ReplayedChild::Restored {
                    parent_native_session_id: spawned.parent_native_session_id.clone(),
                    outcome_known: false,
                }
            };
            let native_session_id = spawned.native_session_id.clone();
            if !stored {
                self.subagent_spawned(spawned)?;
            }
            children.insert(native_session_id, child);
            return Ok(());
        }
        let Some(ReplayedChild::Restored { outcome_known, .. }) =
            children.get_mut(native_session_id(&event))
        else {
            return Ok(());
        };
        match event {
            AgentReplayedSubagentEvent::Spawned(_) => Ok(()),
            AgentReplayedSubagentEvent::Update {
                native_session_id,
                event,
            } => self.subagent_session_update(&native_session_id, event),
            AgentReplayedSubagentEvent::State(update) => {
                *outcome_known = update.state != AgentNativeSubagentState::Running;
                self.subagent_state_changed(update)
            }
            AgentReplayedSubagentEvent::Details(update) => self.subagent_details_changed(update),
            AgentReplayedSubagentEvent::Identity(update) => self.subagent_identity_changed(update),
        }
    }

    fn log_replayed_child_failure(&self, error: &RuntimeError) {
        crate::logging::warn(
            "task_subagent_replay_child_failed",
            serde_json::json!({
                "task_id": self.task_id,
                "error_kind": error.code(),
            }),
        );
    }
}

fn native_session_id(event: &AgentReplayedSubagentEvent) -> &str {
    match event {
        AgentReplayedSubagentEvent::Spawned(event) => &event.native_session_id,
        AgentReplayedSubagentEvent::Update {
            native_session_id, ..
        } => native_session_id,
        AgentReplayedSubagentEvent::State(event) => &event.native_session_id,
        AgentReplayedSubagentEvent::Details(event) => &event.native_session_id,
        AgentReplayedSubagentEvent::Identity(event) => &event.native_session_id,
    }
}
