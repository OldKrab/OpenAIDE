//! The released ACP subagents form (schema 1.10+, `unstable_subagents`).
//!
//! `subagent_update` on a parent session announces a child the first time and
//! patches it afterwards: an omitted field is unchanged and `null` clears it.
//! A child is reusable, so its state can return from idle to running when its
//! parent messages it again. `session_message` and `session_message_chunk` on
//! the child carry what another session sent it, which opens each run with the
//! child's User message.
//!
//! A parent's own session messages to and from its children become
//! delegation Tool rows in the parent's transcript; see
//! `acp_native_subagents_delegation`.

use std::sync::Arc;

use super::{claude_details, native_state_name, AcpNativeSubagentRouter, ChildRoute};
use super::{RoutedSessionNotification, SubagentEventSink, SUBAGENT_NOT_OFFERED};
use crate::agent::acp_live_prompt_projection::LivePromptProjection;
use crate::agent::acp_native_subagents_delegation::{
    counterpart, description_patch, released_state, subagent_name, ChildDelegation, DelegationRow,
    UNTITLED_SUBAGENT,
};
use crate::agent::acp_schema::{
    ContentBlock, MaybeUndefined, SessionId, SessionNotification, StateUpdate, SubagentUpdate,
};
use crate::agent::events::{
    AgentNativeSubagentCapabilities, AgentNativeSubagentIdentityUpdate, AgentNativeSubagentSpawned,
    AgentNativeSubagentState, AgentNativeSubagentStateUpdate,
};
use crate::agent::AgentEventSink;
use crate::protocol::errors::RuntimeError;

/// One session message on `session_id`, in either released shape.
pub(super) struct ReleasedSessionMessage {
    pub(super) session_id: String,
    pub(super) message_id: String,
    pub(super) sender_session_id: Option<SessionId>,
    pub(super) recipient_session_id: Option<SessionId>,
    pub(super) content: Vec<ContentBlock>,
    /// A `session_message_chunk`: part of a message, not all of it.
    pub(super) chunk: bool,
}

impl AcpNativeSubagentRouter {
    /// Applies one `subagent_update` that `parent_session_id` reported. A
    /// terminal state closes the child's open launch row in the parent.
    pub(super) fn route_released_update(
        &self,
        parent_session_id: String,
        update: SubagentUpdate,
    ) -> Result<RoutedSessionNotification, RuntimeError> {
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
        let close = match update.state {
            MaybeUndefined::Value(state) => {
                self.released_state_changed(&parent_session_id, &child_session_id, &state)?
            }
            // `null` withdraws the state report; the stored status stays.
            MaybeUndefined::Null | MaybeUndefined::Undefined => None,
        };
        match close {
            Some((root_session_id, row)) => self.deliver_delegation_row(
                &parent_session_id,
                &root_session_id,
                &child_session_id,
                row,
            ),
            None => Ok(RoutedSessionNotification::Handled),
        }
    }

    /// Builds the delegation row for a session message between `session_id`
    /// and one of its own children. `None` when the message is not such a
    /// delegation and `session_id` is a child that may receive it as its own
    /// message instead. A root session's other messages are logged and dropped.
    pub(super) fn route_delegation_message(
        &self,
        message: &ReleasedSessionMessage,
    ) -> Result<Option<RoutedSessionNotification>, RuntimeError> {
        let session_id = message.session_id.as_str();
        let sender = message.sender_session_id.as_ref().map(|id| &*id.0);
        let recipient = message.recipient_session_id.as_ref().map(|id| &*id.0);
        let counterpart = counterpart(session_id, sender, recipient);
        let mut state = self.inner.lock().expect("ACP Subagent router poisoned");
        let session_is_child = state.children.contains_key(session_id);
        // `own`: the message is between this session and its own child, so
        // it is never also this session's incoming User message.
        let (reason, own) = match counterpart {
            None => ("unaddressed", false),
            Some((child_session_id, outgoing)) => match state.children.get_mut(child_session_id) {
                None if outgoing => ("unknown_recipient", false),
                None => ("unknown_sender", false),
                Some(route) if route.parent_session_id != session_id => ("other_parent", false),
                // The adapter sends a delegation whole; a part cannot open or
                // complete a row on its own.
                Some(_) if message.chunk => ("chunked_message", true),
                Some(route) => match route.delegation.message_row(
                    &route.name,
                    outgoing,
                    &message.message_id,
                    message.content.clone(),
                ) {
                    Ok(row) => {
                        let root_session_id = route.root_session_id.clone();
                        drop(state);
                        return self
                            .deliver_delegation_row(
                                session_id,
                                &root_session_id,
                                child_session_id,
                                row,
                            )
                            .map(Some);
                    }
                    Err(reason) => (reason, true),
                },
            },
        };
        drop(state);
        if session_is_child && !own {
            return Ok(None);
        }
        crate::logging::info(
            "acp_subagent_message_ignored",
            serde_json::json!({
                "session_id": session_id,
                "subagent_session_id": counterpart.map(|(child, _)| child),
                "reason": reason,
            }),
        );
        Ok(Some(RoutedSessionNotification::Handled))
    }

    /// Puts a delegation row into `parent_session_id`'s transcript. A child
    /// parent's row goes to its own history projection. A root parent's row
    /// returns as a root notification, so it takes the single ordered root
    /// pipeline (live projection, persistence, Tool status logging) instead of
    /// being emitted beside it.
    fn deliver_delegation_row(
        &self,
        parent_session_id: &str,
        root_session_id: &str,
        child_session_id: &str,
        row: DelegationRow,
    ) -> Result<RoutedSessionNotification, RuntimeError> {
        crate::logging::info(
            "acp_subagent_delegation_projected",
            serde_json::json!({
                "root_session_id": root_session_id,
                "subagent_session_id": child_session_id,
                "kind": row.kind.name(),
                "status": row.status(),
            }),
        );
        let parent_projection = self
            .inner
            .lock()
            .expect("ACP Subagent router poisoned")
            .children
            .get(parent_session_id)
            .map(|route| route.projection.clone());
        match parent_projection {
            Some(projection) => {
                projection.emit(row.update)?;
                Ok(RoutedSessionNotification::Handled)
            }
            None => Ok(RoutedSessionNotification::Root(Box::new(
                SessionNotification::new(parent_session_id.to_string(), row.update),
            ))),
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
            // TODO: an adapter that hides the child's SubagentHandback call
            // sends its report only as this outgoing view, so the child
            // history loses it. Render it as the child's own row once that
            // presentation is agreed in docs/task-chat-flow.md.
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
    /// child the Task already stores, for example after a load or resume,
    /// reconciles to its stored Subagent on the Task side.
    fn announce_released(
        &self,
        parent_session_id: &str,
        child_session_id: &str,
        update: &SubagentUpdate,
    ) -> Result<(), RuntimeError> {
        let (root_session_id, root_sink) = self.root_for_parent(parent_session_id)?;
        // A stored child keeps what this update omits: after `session/load` the
        // Agent reports each child's state alone.
        let stored = root_sink.stored_subagent_identity(child_session_id)?;
        let reconnected = stored.is_some();
        let name = match (&update.title, &stored) {
            (MaybeUndefined::Undefined, Some(stored)) => stored.name.clone(),
            (title, _) => subagent_name(title.value().map(String::as_str)),
        };
        let delegated_task = description_patch(&update.description)
            .unwrap_or_else(|| stored.and_then(|stored| stored.delegated_task));
        let cancel = update
            .capabilities
            .value()
            .is_some_and(|capabilities| capabilities.cancel.is_some());
        root_sink.subagent_spawned(AgentNativeSubagentSpawned {
            parent_native_session_id: parent_session_id.to_string(),
            native_session_id: child_session_id.to_string(),
            name: name.clone(),
            delegated_task: delegated_task.clone(),
            parent_interaction: false,
            // The parent transcript records the delegation: as the Agent's own
            // tool call, or as the row App Server builds from the parent's
            // session messages when the adapter hides that call.
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
        let delegation = if reconnected {
            ChildDelegation::reconnected(delegated_task.clone())
        } else {
            ChildDelegation::new(delegated_task.clone())
        };
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
                "reconnected": reconnected,
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
                delegation,
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
        if let Some(route) = self
            .inner
            .lock()
            .expect("ACP Subagent router poisoned")
            .children
            .get_mut(child_session_id)
        {
            if let Some(name) = &name {
                route.name = name.clone();
            }
            if let Some(description) = description_patch(&update.description) {
                route.delegation.description = description;
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

    /// Applies a reported work state. Returns the row that closes the child's
    /// open launch when the work ended, with the root session id.
    fn released_state_changed(
        &self,
        parent_session_id: &str,
        child_session_id: &str,
        state: &StateUpdate,
    ) -> Result<Option<(String, DelegationRow)>, RuntimeError> {
        let (next, stop_reason) = released_state(state);
        let Some(next) = next else {
            crate::logging::warn(
                "acp_subagent_state_unrecognized",
                serde_json::json!({
                    "subagent_session_id": child_session_id,
                }),
            );
            return Ok(None);
        };
        let (root_session_id, previous, run_duration_ms, close) = {
            let mut router = self.inner.lock().expect("ACP Subagent router poisoned");
            let route = router.children.get_mut(child_session_id).ok_or_else(|| {
                RuntimeError::InvalidParams("unknown ACP Subagent lifecycle".to_string())
            })?;
            let previous = route.state.replace(next);
            if previous == Some(next) {
                // `running` and `requires_action` share one product status.
                return Ok(None);
            }
            let run_duration_ms = (previous == Some(AgentNativeSubagentState::Running))
                .then(|| route.started_at.elapsed().as_millis());
            if next == AgentNativeSubagentState::Running {
                route.started_at = std::time::Instant::now();
            }
            let close = route.delegation.close_row(next);
            (
                route.root_session_id.clone(),
                previous,
                run_duration_ms,
                close,
            )
        };
        let root_sink = self
            .sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .get(&root_session_id)
            .cloned()
            .ok_or_else(|| RuntimeError::NotReady("Task event sink is unavailable".to_string()))?;
        root_sink.subagent_state_changed(AgentNativeSubagentStateUpdate {
            parent_native_session_id: parent_session_id.to_string(),
            native_session_id: child_session_id.to_string(),
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
        Ok(close.map(|row| (root_session_id, row)))
    }
}

#[cfg(test)]
#[path = "acp_native_subagents_released_tests.rs"]
mod tests;
