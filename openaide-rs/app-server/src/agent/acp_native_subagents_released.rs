//! The released ACP subagents form (schema 1.10+, `unstable_subagents`).
//!
//! `subagent_update` on a parent session announces a child the first time and
//! patches it afterwards: an omitted field is unchanged and `null` clears it.
//! A child is reusable, so its state can return from idle to running when its
//! parent messages it again. `session_message` and `session_message_chunk` on
//! the child carry what another session sent it, which opens each run with the
//! child's User message.

use std::sync::Arc;

use super::{claude_details, native_state_name, AcpNativeSubagentRouter, ChildRoute};
use super::{SubagentEventSink, SUBAGENT_NOT_OFFERED};
use crate::agent::acp_live_prompt_projection::LivePromptProjection;
use crate::agent::acp_schema::{
    ContentBlock, IdleStopReason, MaybeUndefined, SessionId, StateUpdate, SubagentUpdate,
};
use crate::agent::events::{
    AgentNativeSubagentCapabilities, AgentNativeSubagentIdentityUpdate, AgentNativeSubagentSpawned,
    AgentNativeSubagentState, AgentNativeSubagentStateUpdate,
};
use crate::agent::AgentEventSink;
use crate::protocol::errors::RuntimeError;

/// The name a child gets when its parent gives it no title.
const UNTITLED_SUBAGENT: &str = "Subagent";

/// One message another session sent to a child, in either released shape.
pub(super) struct ReleasedSessionMessage {
    pub(super) session_id: String,
    pub(super) message_id: String,
    pub(super) sender_session_id: Option<SessionId>,
    pub(super) recipient_session_id: Option<SessionId>,
    pub(super) content: Vec<ContentBlock>,
}

impl AcpNativeSubagentRouter {
    /// Applies one `subagent_update` that `parent_session_id` reported.
    pub(super) fn route_released_update(
        &self,
        parent_session_id: String,
        update: SubagentUpdate,
    ) -> Result<(), RuntimeError> {
        let child_session_id = update.session_id.to_string();
        let known_parent = self.known_child_parent(&child_session_id)?;
        match known_parent {
            Some(expected) if expected != parent_session_id => {
                return Err(RuntimeError::InvalidParams(
                    "ACP Subagent update has the wrong parent".to_string(),
                ));
            }
            Some(_) => self.patch_released_identity(&child_session_id, &update)?,
            None => self.announce_released(&parent_session_id, &child_session_id, &update)?,
        }
        match update.state {
            MaybeUndefined::Value(state) => {
                self.released_state_changed(parent_session_id, child_session_id, &state)
            }
            // `null` withdraws the state report; the stored status stays.
            MaybeUndefined::Null | MaybeUndefined::Undefined => Ok(()),
        }
    }

    /// Projects a message sent to a known child as that child's User message.
    /// Returns false when the session is not a child this router announced.
    pub(super) fn route_released_message(
        &self,
        message: ReleasedSessionMessage,
    ) -> Result<bool, RuntimeError> {
        let route = self
            .inner
            .lock()
            .expect("ACP Subagent router poisoned")
            .children
            .get(&message.session_id)
            .cloned();
        let Some(route) = route else {
            return Ok(false);
        };
        let outgoing = message
            .sender_session_id
            .as_ref()
            .is_some_and(|sender| *sender.0 == *message.session_id);
        let addressed_elsewhere = message
            .recipient_session_id
            .as_ref()
            .is_some_and(|recipient| *recipient.0 != *message.session_id);
        if outgoing || addressed_elsewhere {
            // The child's own Tool rows already show what it sent.
            crate::logging::info(
                "acp_subagent_message_ignored",
                serde_json::json!({
                    "root_session_id": route.root_session_id,
                    "subagent_session_id": message.session_id,
                    "reason": if outgoing { "outgoing" } else { "other_recipient" },
                }),
            );
            return Ok(true);
        }
        let text = message
            .content
            .into_iter()
            .filter_map(|block| match block {
                ContentBlock::Text(text) => Some(text.text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            route
                .projection
                .emit_user_prompt(text, Some(message.message_id))?;
        }
        Ok(true)
    }

    /// The parent a known child was announced on, or `None` for a new child.
    fn known_child_parent(&self, child_session_id: &str) -> Result<Option<String>, RuntimeError> {
        let state = self.inner.lock().expect("ACP Subagent router poisoned");
        if !state.negotiated {
            return Err(RuntimeError::InvalidParams(
                SUBAGENT_NOT_OFFERED.to_string(),
            ));
        }
        Ok(state
            .children
            .get(child_session_id)
            .map(|route| route.parent_session_id.clone()))
    }

    /// The first update for a child this connection has not routed yet. A
    /// child the Task already stores, for example after a resume, reconciles
    /// to its stored Subagent on the Task side.
    fn announce_released(
        &self,
        parent_session_id: &str,
        child_session_id: &str,
        update: &SubagentUpdate,
    ) -> Result<(), RuntimeError> {
        let (root_session_id, root_sink) = self.root_for_parent(parent_session_id)?;
        let name = update
            .title
            .value()
            .map(|title| title.trim())
            .filter(|title| !title.is_empty())
            .unwrap_or(UNTITLED_SUBAGENT)
            .to_string();
        let delegated_task = update
            .description
            .value()
            .map(|description| description.trim().to_string())
            .filter(|description| !description.is_empty());
        let cancel = update
            .capabilities
            .value()
            .is_some_and(|capabilities| capabilities.cancel.is_some());
        root_sink.subagent_spawned(AgentNativeSubagentSpawned {
            parent_native_session_id: parent_session_id.to_string(),
            native_session_id: child_session_id.to_string(),
            name: name.clone(),
            delegated_task,
            parent_interaction: false,
            parent_records_delegation: true,
            capabilities: AgentNativeSubagentCapabilities {
                cancel,
                close: false,
            },
            details: claude_details(update.meta.value()),
        })?;
        let event_sink: Arc<dyn AgentEventSink> = Arc::new(SubagentEventSink {
            native_session_id: child_session_id.to_string(),
            root_session_id: root_session_id.clone(),
            sinks: self.sinks.clone(),
        });
        let projection =
            LivePromptProjection::for_native_subagent(&self.agent_id, event_sink, false);
        let mut state = self.inner.lock().expect("ACP Subagent router poisoned");
        if !state.negotiated {
            return Err(RuntimeError::InvalidParams(
                SUBAGENT_NOT_OFFERED.to_string(),
            ));
        }
        crate::logging::info(
            "acp_subagent_session_started",
            serde_json::json!({
                "form": "released",
                "root_session_id": root_session_id,
                "parent_session_id": parent_session_id,
                "subagent_session_id": child_session_id,
                "cancel_capability": cancel,
            }),
        );
        state
            .children
            .entry(child_session_id.to_string())
            .or_insert(ChildRoute {
                parent_session_id: parent_session_id.to_string(),
                root_session_id,
                name,
                projection,
                started_at: std::time::Instant::now(),
                state: None,
            });
        Ok(())
    }

    fn patch_released_identity(
        &self,
        child_session_id: &str,
        update: &SubagentUpdate,
    ) -> Result<(), RuntimeError> {
        let name = match &update.title {
            MaybeUndefined::Value(title) if !title.trim().is_empty() => {
                Some(title.trim().to_string())
            }
            MaybeUndefined::Value(_) | MaybeUndefined::Null => Some(UNTITLED_SUBAGENT.to_string()),
            MaybeUndefined::Undefined => None,
        };
        let delegated_task = match &update.description {
            MaybeUndefined::Value(description) => Some(description.trim().to_string()),
            MaybeUndefined::Null => Some(String::new()),
            MaybeUndefined::Undefined => None,
        };
        let cancel = match &update.capabilities {
            MaybeUndefined::Value(capabilities) => Some(capabilities.cancel.is_some()),
            MaybeUndefined::Null => Some(false),
            MaybeUndefined::Undefined => None,
        };
        if update.meta.is_value() {
            self.details_changed(child_session_id.to_string(), update.meta.value())?;
        }
        if name.is_none() && delegated_task.is_none() && cancel.is_none() {
            return Ok(());
        }
        if let Some(name) = &name {
            if let Some(route) = self
                .inner
                .lock()
                .expect("ACP Subagent router poisoned")
                .children
                .get_mut(child_session_id)
            {
                route.name = name.clone();
            }
        }
        let Some((_, root_sink)) = self.root_sink_for_child(child_session_id) else {
            return Err(RuntimeError::NotReady(
                "Task event sink is unavailable".to_string(),
            ));
        };
        root_sink.subagent_identity_changed(AgentNativeSubagentIdentityUpdate {
            native_session_id: child_session_id.to_string(),
            name,
            delegated_task,
            cancel,
        })
    }

    fn released_state_changed(
        &self,
        parent_session_id: String,
        child_session_id: String,
        state: &StateUpdate,
    ) -> Result<(), RuntimeError> {
        let (next, stop_reason) = released_state(state);
        let Some(next) = next else {
            crate::logging::warn(
                "acp_subagent_state_unrecognized",
                serde_json::json!({
                    "subagent_session_id": child_session_id,
                }),
            );
            return Ok(());
        };
        let (root_session_id, previous, run_duration_ms) = {
            let mut router = self.inner.lock().expect("ACP Subagent router poisoned");
            let route = router.children.get_mut(&child_session_id).ok_or_else(|| {
                RuntimeError::InvalidParams("unknown ACP Subagent lifecycle".to_string())
            })?;
            let previous = route.state.replace(next);
            if previous == Some(next) {
                // `running` and `requires_action` share one product status.
                return Ok(());
            }
            let run_duration_ms = (previous == Some(AgentNativeSubagentState::Running))
                .then(|| route.started_at.elapsed().as_millis());
            if next == AgentNativeSubagentState::Running {
                route.started_at = std::time::Instant::now();
            }
            (route.root_session_id.clone(), previous, run_duration_ms)
        };
        let root_sink = self
            .sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .get(&root_session_id)
            .cloned()
            .ok_or_else(|| RuntimeError::NotReady("Task event sink is unavailable".to_string()))?;
        root_sink.subagent_state_changed(AgentNativeSubagentStateUpdate {
            parent_native_session_id: parent_session_id,
            native_session_id: child_session_id.clone(),
            state: next,
        })?;
        crate::logging::info(
            "acp_subagent_state_changed",
            serde_json::json!({
                "root_session_id": root_session_id,
                "subagent_session_id": child_session_id,
                "from": previous.map(native_state_name),
                "to": native_state_name(next),
                "stop_reason": stop_reason,
                "run_duration_ms": run_duration_ms,
            }),
        );
        Ok(())
    }
}

/// Maps a released work state to the product status and a stop-reason
/// category for logs. `None` means the state kind is not one this build knows.
fn released_state(state: &StateUpdate) -> (Option<AgentNativeSubagentState>, Option<&'static str>) {
    match state {
        StateUpdate::Running(_) | StateUpdate::RequiresAction(_) => {
            (Some(AgentNativeSubagentState::Running), None)
        }
        StateUpdate::Idle(idle) => {
            let (status, reason) = match &idle.stop_reason {
                // Idle without a reason: the work ended without a reported failure.
                None => (AgentNativeSubagentState::Completed, "none"),
                Some(IdleStopReason::EndTurn) => (AgentNativeSubagentState::Completed, "end_turn"),
                Some(IdleStopReason::Cancelled) => {
                    (AgentNativeSubagentState::Cancelled, "cancelled")
                }
                Some(IdleStopReason::Error(_)) => (AgentNativeSubagentState::Failed, "error"),
                Some(IdleStopReason::Refusal) => (AgentNativeSubagentState::Failed, "refusal"),
                Some(IdleStopReason::MaxTokens) => (AgentNativeSubagentState::Failed, "max_tokens"),
                Some(IdleStopReason::MaxTurnRequests) => {
                    (AgentNativeSubagentState::Failed, "max_turn_requests")
                }
                Some(_) => (AgentNativeSubagentState::Failed, "unrecognized"),
            };
            (Some(status), Some(reason))
        }
        StateUpdate::Unknown(_) => (Some(AgentNativeSubagentState::Disconnected), None),
        _ => (None, None),
    }
}

#[cfg(test)]
#[path = "acp_native_subagents_released_tests.rs"]
mod tests;
