//! Subagent writes as data, so the Task journal worker can apply them in the
//! Task's queue order (ADR-0076). Callers submit a [`SubagentWrite`]; nothing
//! here touches the disk.

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStep, AgentMessagePart, AgentPlan, ChatMessage, NormalizedMessage,
    ToolPermissionOutcome,
};
use crate::storage::cursor;
use crate::storage::records::StoredMessage;

use super::stream::{advance_history_meta, HistoryChange, SubagentChat};
use super::{
    SubagentCapabilitiesRecord, SubagentDetailRecord, SubagentRecord, SubagentStatusRecord,
};

/// One change to a Subagent of a Task, addressed by its native session id.
#[derive(Debug, Clone)]
pub(crate) struct SubagentWrite {
    pub(crate) native_session_id: String,
    pub(crate) operation: SubagentOperation,
}

#[derive(Debug, Clone)]
pub(crate) enum SubagentOperation {
    /// Reconciles one Agent-announced lifecycle generation; a known native
    /// session keeps its record.
    Spawn {
        parent_native_session_id: String,
        name: String,
        delegated_task: String,
        capabilities: SubagentCapabilitiesRecord,
        details: Vec<SubagentDetailRecord>,
    },
    SetStatus(SubagentStatusRecord),
    /// `None` leaves a field as it is.
    UpdateIdentity {
        name: Option<String>,
        delegated_task: Option<String>,
        cancel: Option<bool>,
    },
    /// Replaces details that share a label and appends the rest.
    MergeDetails(Vec<SubagentDetailRecord>),
    /// Appends a message, or with `upsert` replaces the one with its identity.
    AppendMessage {
        message: NormalizedMessage,
        upsert: bool,
    },
    /// Appends one streamed chunk to the message with its identity.
    AppendMessagePart(NormalizedMessage),
    SetPlan(AgentPlan),
    RecordPermissionOutcome {
        tool_call_id: String,
        outcome: ToolPermissionOutcome,
    },
}

impl SubagentWrite {
    pub(crate) fn estimated_bytes(&self) -> usize {
        let payload = match &self.operation {
            SubagentOperation::AppendMessage { message, .. }
            | SubagentOperation::AppendMessagePart(message) => {
                serde_json::to_vec(message).map_or(usize::MAX, |bytes| bytes.len())
            }
            SubagentOperation::SetPlan(plan) => {
                serde_json::to_vec(plan).map_or(usize::MAX, |bytes| bytes.len())
            }
            _ => 512,
        };
        payload.saturating_add(self.native_session_id.len())
    }

    /// A stable name for diagnostics.
    pub(crate) fn operation_kind(&self) -> &'static str {
        match self.operation {
            SubagentOperation::Spawn { .. } => "spawn",
            SubagentOperation::SetStatus(_) => "set_status",
            SubagentOperation::UpdateIdentity { .. } => "update_identity",
            SubagentOperation::MergeDetails(_) => "merge_details",
            SubagentOperation::AppendMessage { .. } => "append_message",
            SubagentOperation::AppendMessagePart(_) => "append_message_part",
            SubagentOperation::SetPlan(_) => "set_plan",
            SubagentOperation::RecordPermissionOutcome { .. } => "record_permission_outcome",
        }
    }

    /// Whether the write changes a Subagent History rather than only its catalog entry.
    pub(super) fn changes_history(&self) -> bool {
        matches!(
            self.operation,
            SubagentOperation::AppendMessage { .. }
                | SubagentOperation::AppendMessagePart(_)
                | SubagentOperation::SetPlan(_)
                | SubagentOperation::RecordPermissionOutcome { .. }
        )
    }
}

/// Applies a catalog-only operation to its record.
pub(super) fn apply_to_record(record: &mut SubagentRecord, operation: SubagentOperation) {
    match operation {
        SubagentOperation::SetStatus(status) => record.status = status,
        SubagentOperation::UpdateIdentity {
            name,
            delegated_task,
            cancel,
        } => {
            if let Some(name) = name {
                record.name = name;
            }
            if let Some(delegated_task) = delegated_task {
                record.delegated_task = delegated_task;
            }
            if let Some(cancel) = cancel {
                record.capabilities.cancel = cancel;
            }
        }
        SubagentOperation::MergeDetails(details) => {
            for detail in details {
                match record
                    .details
                    .iter_mut()
                    .find(|stored| stored.label == detail.label)
                {
                    Some(stored) => stored.value = detail.value,
                    None => record.details.push(detail),
                }
            }
        }
        SubagentOperation::Spawn { .. }
        | SubagentOperation::AppendMessage { .. }
        | SubagentOperation::AppendMessagePart(_)
        | SubagentOperation::SetPlan(_)
        | SubagentOperation::RecordPermissionOutcome { .. } => {
            unreachable!("history and spawn operations are not record patches")
        }
    }
}

/// Applies a history operation in memory and returns the change to journal.
pub(super) fn apply_to_history(
    history: &mut SubagentChat,
    operation: SubagentOperation,
) -> Result<HistoryChange, RuntimeError> {
    match operation {
        SubagentOperation::AppendMessage { message, upsert } => {
            Ok(append_message(history, message, upsert))
        }
        SubagentOperation::AppendMessagePart(message) => append_message_part(history, message),
        SubagentOperation::SetPlan(plan) => {
            history.current_plan = (!plan.entries.is_empty()).then_some(plan);
            Ok(HistoryChange::Plan)
        }
        SubagentOperation::RecordPermissionOutcome {
            tool_call_id,
            outcome,
        } => record_permission_outcome(history, &tool_call_id, outcome),
        SubagentOperation::Spawn { .. }
        | SubagentOperation::SetStatus(_)
        | SubagentOperation::UpdateIdentity { .. }
        | SubagentOperation::MergeDetails(_) => {
            unreachable!("catalog operations do not change a history")
        }
    }
}

fn append_message(
    history: &mut SubagentChat,
    mut message: NormalizedMessage,
    upsert: bool,
) -> HistoryChange {
    let identity = message.identity();
    if upsert {
        if let Some(stored) = history
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == identity)
        {
            message.preserve_created_at_from(&stored.chat.message);
            stored.chat.message = message;
            advance_history_meta(history);
            return HistoryChange::Upsert(identity);
        }
    }
    push_message(history, identity, message)
}

fn append_message_part(
    history: &mut SubagentChat,
    message: NormalizedMessage,
) -> Result<HistoryChange, RuntimeError> {
    let identity = message.identity();
    let Some(stored) = history
        .messages
        .iter_mut()
        .find(|stored| stored.chat.identity == identity)
    else {
        return Ok(push_message(history, identity, message));
    };
    let change = match (&mut stored.chat.message, message) {
        (
            NormalizedMessage::AgentMessage { role, parts, .. },
            NormalizedMessage::AgentMessage {
                role: incoming_role,
                parts: incoming_parts,
                ..
            },
        ) if *role == incoming_role && incoming_parts.len() == 1 => {
            let part = incoming_parts.into_iter().next().expect("one checked part");
            if let AgentMessagePart::Text { text: chunk } = part {
                if let Some(AgentMessagePart::Text { text }) = parts.last_mut() {
                    text.push_str(&chunk);
                } else {
                    parts.push(AgentMessagePart::Text {
                        text: chunk.clone(),
                    });
                }
                HistoryChange::AppendText {
                    identity,
                    text: chunk,
                }
            } else {
                parts.push(part);
                HistoryChange::Upsert(identity)
            }
        }
        (
            NormalizedMessage::User {
                text, attachments, ..
            },
            NormalizedMessage::User {
                text: chunk,
                attachments: incoming_attachments,
                ..
            },
        ) => {
            text.push_str(&chunk);
            attachments.extend(incoming_attachments);
            HistoryChange::Upsert(identity)
        }
        _ => {
            return Err(RuntimeError::Storage(
                "Subagent message chunk does not match its stored message".to_string(),
            ))
        }
    };
    advance_history_meta(history);
    Ok(change)
}

fn push_message(
    history: &mut SubagentChat,
    identity: String,
    message: NormalizedMessage,
) -> HistoryChange {
    let sequence = history
        .messages
        .last()
        .map(|stored| stored.sequence.saturating_add(1))
        .unwrap_or(1);
    history.messages.push(StoredMessage {
        sequence,
        chat: ChatMessage {
            cursor: cursor::from_sequence(sequence),
            message_id: identity.clone(),
            identity: identity.clone(),
            message_type: message.message_type().to_string(),
            message,
            timing: Default::default(),
        },
    });
    advance_history_meta(history);
    HistoryChange::Upsert(identity)
}

fn record_permission_outcome(
    history: &mut SubagentChat,
    tool_call_id: &str,
    outcome: ToolPermissionOutcome,
) -> Result<HistoryChange, RuntimeError> {
    let Some((identity, outcomes)) = history.messages.iter_mut().find_map(|stored| {
        let chat = &mut stored.chat;
        let NormalizedMessage::Activity { steps, .. } = &mut chat.message else {
            return None;
        };
        steps
            .iter_mut()
            .find_map(|step| match step {
                ActivityStep::Tool {
                    tool_call_id: Some(id),
                    permission_outcomes,
                    ..
                } if id == tool_call_id => Some(permission_outcomes),
                _ => None,
            })
            .map(|outcomes| (chat.identity.clone(), outcomes))
    }) else {
        return Err(RuntimeError::Storage(
            "Subagent permission outcome has no matching tool call".to_string(),
        ));
    };
    if let Some(existing) = outcomes
        .iter_mut()
        .find(|existing| existing.request_id == outcome.request_id)
    {
        *existing = outcome;
    } else {
        outcomes.push(outcome);
    }
    advance_history_meta(history);
    Ok(HistoryChange::Upsert(identity))
}
