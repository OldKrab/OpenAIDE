//! Parent-side delegation rows for the released ACP Subagents form.
//!
//! An adapter on the released form may hide the Agent tool call that
//! delegates to a child and send the parent session's side of the exchange as
//! `session_message`s instead: the launch prompt and every later message to
//! the child go out from the parent, and the child's reply (`{launch}:reply`)
//! or hand-back comes in to it. The parent's Chat would then lose the
//! delegation row. App Server rebuilds the generic Tool row the Agent tool
//! call used to produce from those messages.
//!
//! The live router and `session/load` replay share this module, so both build
//! the same rows. Row ids are the message ids (a reply completes its launch's
//! row), which keeps a replayed row identical to the live one. An adapter that
//! shows the Agent tool call sends no parent-side session messages, so a
//! delegation is never rendered twice.

use crate::agent::acp_schema::{
    ContentBlock, IdleStopReason, MaybeUndefined, SessionMessage, SessionUpdate, StateUpdate,
    SubagentUpdate, ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind,
};
use crate::agent::events::AgentNativeSubagentState;

/// The name a child gets when its parent gives it no title.
pub(super) const UNTITLED_SUBAGENT: &str = "Subagent";

/// Suffix the adapter adds to a launch id for the child's answer to it.
const REPLY_SUFFIX: &str = ":reply";

/// What a parent session knows about one child's delegations.
///
/// Invariant: `open_launch` is either `None` or equal to `launch_id`. A child
/// is launched once; later deliveries to it are messages.
#[derive(Clone, Debug, Default)]
pub(super) struct ChildDelegation {
    /// The child's own description, preferred over its name as the row title.
    pub(super) description: Option<String>,
    /// The id of the message that launched the child, once seen.
    launch_id: Option<String>,
    /// The launch row that is still in progress.
    open_launch: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DelegationKind {
    Launch,
    Send,
    Reply,
    Handback,
    Close,
}

impl DelegationKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Send => "send",
            Self::Reply => "reply",
            Self::Handback => "handback",
            Self::Close => "close",
        }
    }
}

/// One Tool row change for the parent session's transcript.
#[derive(Debug)]
pub(super) struct DelegationRow {
    pub(super) kind: DelegationKind,
    pub(super) update: SessionUpdate,
}

impl DelegationRow {
    /// The resulting Tool status, for logs.
    pub(super) fn status(&self) -> Option<&'static str> {
        let status = match &self.update {
            SessionUpdate::ToolCall(call) => Some(call.status),
            SessionUpdate::ToolCallUpdate(update) => update.fields.status,
            _ => None,
        };
        status.map(|status| crate::agent::acp_tool_call_projection::tool_status_name(&status))
    }
}

impl ChildDelegation {
    pub(super) fn new(description: Option<String>) -> Self {
        Self {
            description,
            ..Self::default()
        }
    }

    /// The row for one session message between this child and its parent,
    /// seen on the parent's session. `outgoing` means the parent sent it.
    /// `Err` carries the reason to ignore it, for logs.
    pub(super) fn message_row(
        &mut self,
        name: &str,
        outgoing: bool,
        message_id: &str,
        content: Vec<ContentBlock>,
    ) -> Result<DelegationRow, &'static str> {
        if outgoing {
            if self.launch_id.as_deref() == Some(message_id) {
                return Err("repeated_launch");
            }
            if self.launch_id.is_none() {
                self.launch_id = Some(message_id.to_string());
                self.open_launch = Some(message_id.to_string());
                let title = self.description.as_deref().unwrap_or(name);
                return Ok(DelegationRow {
                    kind: DelegationKind::Launch,
                    update: SessionUpdate::ToolCall(launch_tool_call(message_id, content, title)),
                });
            }
            return Ok(DelegationRow {
                kind: DelegationKind::Send,
                update: SessionUpdate::ToolCall(message_tool_call(
                    message_id,
                    format!("Message to {name}"),
                    "SendMessage",
                    content,
                )),
            });
        }
        if let Some(launch_id) = message_id.strip_suffix(REPLY_SUFFIX) {
            if self.launch_id.as_deref() != Some(launch_id) {
                return Err("reply_without_launch");
            }
            if self.open_launch.as_deref() == Some(launch_id) {
                self.open_launch = None;
            }
            return Ok(DelegationRow {
                kind: DelegationKind::Reply,
                update: SessionUpdate::ToolCallUpdate(reply_update(launch_id, content)),
            });
        }
        let update = match self.open_launch.take() {
            Some(launch_id) => SessionUpdate::ToolCallUpdate(reply_update(&launch_id, content)),
            None => SessionUpdate::ToolCall(message_tool_call(
                message_id,
                format!("Message from {name}"),
                "SubagentHandback",
                content,
            )),
        };
        Ok(DelegationRow {
            kind: DelegationKind::Handback,
            update,
        })
    }

    /// Closes the open launch row when the child's work reaches `state`.
    /// A background launch, a failure, or a run without a hand-back ends this
    /// way; a reply that arrives later still replaces the row's content.
    pub(super) fn close_row(&mut self, state: AgentNativeSubagentState) -> Option<DelegationRow> {
        if state == AgentNativeSubagentState::Running {
            return None;
        }
        let launch_id = self.open_launch.take()?;
        Some(DelegationRow {
            kind: DelegationKind::Close,
            update: SessionUpdate::ToolCallUpdate(close_update(&launch_id, state)),
        })
    }
}

/// The child a session message on `session_id` is exchanged with, and
/// whether `session_id` sent it. `None` when the message names no other
/// session or does not involve `session_id`.
pub(super) fn counterpart<'a>(
    session_id: &str,
    sender: Option<&'a str>,
    recipient: Option<&'a str>,
) -> Option<(&'a str, bool)> {
    match (sender, recipient) {
        (Some(sender), Some(recipient)) if sender == session_id && recipient != session_id => {
            Some((recipient, true))
        }
        (Some(sender), recipient)
            if sender != session_id && recipient.is_none_or(|id| id == session_id) =>
        {
            Some((sender, false))
        }
        _ => None,
    }
}

/// Marks a Tool row that App Server built, so a projection that already holds
/// the Agent's real tool call with that id keeps it instead.
const DELEGATION_META_KEY: &str = "openaideDelegationRow";

/// The launch row: the shape the Agent tool call had, so it renders as the
/// same generic Tool row.
pub(crate) fn launch_tool_call(
    message_id: &str,
    prompt: Vec<ContentBlock>,
    title: &str,
) -> ToolCall {
    ToolCall::new(message_id.to_string(), title.to_string())
        .kind(ToolKind::Think)
        .status(ToolCallStatus::InProgress)
        .content(tool_content(prompt))
        .meta(delegation_meta("Agent"))
}

/// A message row created already completed: delivery is its whole lifecycle.
fn message_tool_call(
    message_id: &str,
    title: String,
    tool_name: &str,
    content: Vec<ContentBlock>,
) -> ToolCall {
    ToolCall::new(message_id.to_string(), title)
        .kind(ToolKind::Other)
        .status(ToolCallStatus::Completed)
        .content(tool_content(content))
        .meta(delegation_meta(tool_name))
}

/// Completes a launch row with the child's answer.
pub(crate) fn reply_update(launch_id: &str, content: Vec<ContentBlock>) -> ToolCallUpdate {
    ToolCallUpdate::new(
        launch_id.to_string(),
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .content(tool_content(content)),
    )
}

/// Ends a launch row with the child's final state. ACP Tool status has no
/// cancelled value, so cancelled and disconnected work is Failed.
pub(super) fn close_update(launch_id: &str, state: AgentNativeSubagentState) -> ToolCallUpdate {
    let status = match state {
        AgentNativeSubagentState::Completed => ToolCallStatus::Completed,
        AgentNativeSubagentState::Running => ToolCallStatus::InProgress,
        AgentNativeSubagentState::Failed
        | AgentNativeSubagentState::Cancelled
        | AgentNativeSubagentState::Disconnected => ToolCallStatus::Failed,
    };
    ToolCallUpdate::new(
        launch_id.to_string(),
        ToolCallUpdateFields::new().status(status),
    )
}

/// Whether `tool_call` is a row this module built.
pub(super) fn is_delegation_row(tool_call: &ToolCall) -> bool {
    tool_call
        .meta
        .as_ref()
        .and_then(|meta| meta.get(DELEGATION_META_KEY))
        .is_some_and(|marker| marker.as_bool() == Some(true))
}

fn delegation_meta(tool_name: &str) -> serde_json::Map<String, serde_json::Value> {
    let serde_json::Value::Object(meta) = serde_json::json!({
        "claudeCode": { "toolName": tool_name },
        DELEGATION_META_KEY: true,
    }) else {
        unreachable!("a JSON object literal is an object");
    };
    meta
}

fn tool_content(content: Vec<ContentBlock>) -> Vec<ToolCallContent> {
    content.into_iter().map(ToolCallContent::from).collect()
}

// The released-form label and state mappings live here, not in the router's
// private `released` module, because load replay needs them too.

/// The child's name from a `title` its parent sent.
pub(super) fn subagent_name(title: Option<&str>) -> String {
    title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(UNTITLED_SUBAGENT)
        .to_string()
}

/// A `description` patch: `None` leaves it unchanged, `Some(None)` clears it.
pub(super) fn description_patch(description: &MaybeUndefined<String>) -> Option<Option<String>> {
    match description {
        MaybeUndefined::Value(description) => {
            Some(Some(description.trim().to_string()).filter(|description| !description.is_empty()))
        }
        MaybeUndefined::Null => Some(None),
        MaybeUndefined::Undefined => None,
    }
}

/// Maps a released work state to the product status and a stop-reason
/// category for logs. `None` means the state kind is not one this build knows.
pub(super) fn released_state(
    state: &StateUpdate,
) -> (Option<AgentNativeSubagentState>, Option<&'static str>) {
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

/// Rebuilds the loaded root session's delegation rows during `session/load`.
/// The replay sees only root updates: the children the root announced and its
/// own session messages. A grandchild's rows belong to a child history, which
/// load replay does not project.
#[derive(Default)]
pub(super) struct ReplayDelegations {
    children: std::collections::HashMap<String, (String, ChildDelegation)>,
    rows: usize,
    ignored: usize,
}

impl ReplayDelegations {
    /// Records a root `subagent_update`; returns a close row when it ends a launch.
    pub(super) fn subagent_update(&mut self, update: &SubagentUpdate) -> Option<SessionUpdate> {
        let (name, delegation) = self
            .children
            .entry(update.session_id.to_string())
            .or_insert_with(|| (subagent_name(None), ChildDelegation::default()));
        match &update.title {
            MaybeUndefined::Value(title) => *name = subagent_name(Some(title)),
            MaybeUndefined::Null => *name = subagent_name(None),
            MaybeUndefined::Undefined => {}
        }
        if let Some(description) = description_patch(&update.description) {
            delegation.description = description;
        }
        let MaybeUndefined::Value(state) = &update.state else {
            return None;
        };
        let state = released_state(state).0?;
        let row = delegation.close_row(state)?;
        self.rows += 1;
        Some(row.update)
    }

    /// The row for a root session message, when it is a delegation with a
    /// child the replay announced.
    pub(super) fn session_message(
        &mut self,
        session_id: &str,
        message: SessionMessage,
    ) -> Option<SessionUpdate> {
        let sender = message.sender_session_id.as_ref().map(|id| &*id.0);
        let recipient = message.recipient_session_id.as_ref().map(|id| &*id.0);
        let row = counterpart(session_id, sender, recipient)
            .and_then(|(child, outgoing)| Some((self.children.get_mut(child)?, outgoing)))
            .and_then(|((name, delegation), outgoing)| {
                let content = message.content.value().cloned().unwrap_or_default();
                delegation
                    .message_row(name, outgoing, &message.message_id.to_string(), content)
                    .ok()
            });
        match row {
            Some(row) => {
                self.rows += 1;
                Some(row.update)
            }
            None => {
                self.ignored += 1;
                None
            }
        }
    }

    /// How many rows the replay built and how many session messages it
    /// ignored, for its summary log.
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.rows, self.ignored)
    }
}
