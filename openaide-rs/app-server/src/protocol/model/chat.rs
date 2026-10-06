use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use openaide_app_server_protocol::server_requests::{QuestionField, QuestionValue};

use super::{ActivityStatus, ActivityStep, AgentPlanEntry};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessagePage {
    pub task_id: String,
    pub items: Vec<ChatMessage>,
    pub has_before: bool,
    pub total_count: u64,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub cursor: String,
    pub identity: String,
    pub message_type: String,
    pub message_id: String,
    pub message: NormalizedMessage,
    /// App Server-clock observations for this row. Stored beside the message so
    /// an Agent update that replaces the message cannot erase them.
    #[serde(default, skip_serializing_if = "ChatTiming::is_empty")]
    pub timing: ChatTiming,
}

/// Times the App Server witnessed for one Chat row. Each field stays absent
/// unless the App Server saw the moment itself, so rows saved before timing
/// existed and rows rebuilt from a native session carry none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct ChatTiming {
    /// When a user message was accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    /// The row's own work: one Activity or one compaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<ObservedSpan>,
    /// The turn this row closed, set on its final Agent answer or interruption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_turn: Option<ObservedSpan>,
}

impl ChatTiming {
    pub fn is_empty(&self) -> bool {
        self.sent_at.is_none() && self.run.is_none() && self.closed_turn.is_none()
    }
}

/// Epoch-millisecond bounds; `ended_at` is absent while the work is running.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ObservedSpan {
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedMessage {
    User {
        id: String,
        text: String,
        created_at: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<Attachment>,
    },
    AgentMessage {
        id: String,
        role: AgentMessageRole,
        parts: Vec<AgentMessagePart>,
        created_at: String,
    },
    Activity {
        id: String,
        title: String,
        status: ActivityStatus,
        created_at: String,
        collapsed: bool,
        steps: Vec<ActivityStep>,
    },
    CompletedPlan {
        id: String,
        entries: Vec<AgentPlanEntry>,
        created_at: String,
    },
    /// Final incomplete Plan snapshot retained after an explicit user close.
    ClosedPlan {
        id: String,
        entries: Vec<AgentPlanEntry>,
        created_at: String,
    },
    Question {
        id: String,
        request_id: String,
        message: String,
        fields: Vec<QuestionField>,
        state: QuestionState,
        created_at: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<QuestionAction>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<BTreeMap<String, QuestionValue>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resolution_message: Option<String>,
    },
    Interruption {
        id: String,
        reason: InterruptionReason,
        message: String,
        created_at: String,
        recoverable: bool,
    },
    /// One Agent-owned context compaction, updated in place by its ACP
    /// `compactionId`. The summary is the Agent's user-displayable text, never
    /// the model-facing continuation prompt.
    Compaction {
        id: String,
        status: CompactionStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        created_at: String,
    },
}

/// Lifecycle of a context compaction. `Unknown` preserves forward compatibility
/// with ACP statuses this build does not recognize without inferring behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionStatus {
    InProgress,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

impl NormalizedMessage {
    pub fn message_type(&self) -> &'static str {
        match self {
            NormalizedMessage::User { .. } => "user",
            NormalizedMessage::AgentMessage {
                role: AgentMessageRole::Agent,
                ..
            } => "agent_message",
            NormalizedMessage::AgentMessage {
                role: AgentMessageRole::Thought,
                ..
            } => "thought_message",
            NormalizedMessage::Activity { .. } => "activity",
            NormalizedMessage::CompletedPlan { .. } => "completed_plan",
            NormalizedMessage::ClosedPlan { .. } => "closed_plan",
            NormalizedMessage::Question { .. } => "question",
            NormalizedMessage::Interruption { .. } => "interruption",
            NormalizedMessage::Compaction { .. } => "compaction",
        }
    }

    pub fn identity(&self) -> String {
        match self {
            NormalizedMessage::User { id, .. }
            | NormalizedMessage::AgentMessage { id, .. }
            | NormalizedMessage::Activity { id, .. }
            | NormalizedMessage::CompletedPlan { id, .. }
            | NormalizedMessage::ClosedPlan { id, .. }
            | NormalizedMessage::Question { id, .. }
            | NormalizedMessage::Interruption { id, .. }
            | NormalizedMessage::Compaction { id, .. } => id.clone(),
        }
    }

    /// Whether the row represents work that has not settled yet.
    pub fn is_in_progress(&self) -> bool {
        match self {
            NormalizedMessage::Activity { status, .. } => *status == ActivityStatus::Running,
            NormalizedMessage::Compaction { status, .. } => *status == CompactionStatus::InProgress,
            _ => false,
        }
    }

    pub fn created_at(&self) -> &str {
        match self {
            NormalizedMessage::User { created_at, .. }
            | NormalizedMessage::AgentMessage { created_at, .. }
            | NormalizedMessage::Activity { created_at, .. }
            | NormalizedMessage::CompletedPlan { created_at, .. }
            | NormalizedMessage::ClosedPlan { created_at, .. }
            | NormalizedMessage::Question { created_at, .. }
            | NormalizedMessage::Interruption { created_at, .. }
            | NormalizedMessage::Compaction { created_at, .. } => created_at,
        }
    }

    pub fn preserve_created_at_from(&mut self, existing: &NormalizedMessage) {
        let existing_created_at = match existing {
            NormalizedMessage::User { created_at, .. }
            | NormalizedMessage::AgentMessage { created_at, .. }
            | NormalizedMessage::Activity { created_at, .. }
            | NormalizedMessage::CompletedPlan { created_at, .. }
            | NormalizedMessage::ClosedPlan { created_at, .. }
            | NormalizedMessage::Question { created_at, .. }
            | NormalizedMessage::Interruption { created_at, .. }
            | NormalizedMessage::Compaction { created_at, .. } => created_at.clone(),
        };
        match self {
            NormalizedMessage::User { created_at, .. }
            | NormalizedMessage::AgentMessage { created_at, .. }
            | NormalizedMessage::Activity { created_at, .. }
            | NormalizedMessage::CompletedPlan { created_at, .. }
            | NormalizedMessage::ClosedPlan { created_at, .. }
            | NormalizedMessage::Question { created_at, .. }
            | NormalizedMessage::Interruption { created_at, .. }
            | NormalizedMessage::Compaction { created_at, .. } => *created_at = existing_created_at,
        }
    }

    /// ACP tool updates replace the same activity row, while authorization outcomes
    /// are App Server-owned history and must survive those replacements.
    pub fn preserve_tool_permission_outcomes_from(&mut self, existing: &NormalizedMessage) {
        let (
            NormalizedMessage::Activity { steps, .. },
            NormalizedMessage::Activity {
                steps: existing_steps,
                ..
            },
        ) = (self, existing)
        else {
            return;
        };
        for step in steps {
            let super::ActivityStep::Tool {
                tool_call_id,
                permission_outcomes,
                ..
            } = step
            else {
                continue;
            };
            let Some(existing_outcomes) = existing_steps.iter().find_map(|existing_step| {
                let super::ActivityStep::Tool {
                    tool_call_id: existing_id,
                    permission_outcomes,
                    ..
                } = existing_step
                else {
                    return None;
                };
                (existing_id == tool_call_id).then_some(permission_outcomes)
            }) else {
                continue;
            };
            *permission_outcomes = existing_outcomes.clone();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageRole {
    Agent,
    Thought,
}

/// App Server-owned representation of displayable ACP content.
/// Reserved ACP metadata and annotations intentionally do not cross this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentMessagePart {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        data: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
    },
    Resource {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        size_bytes: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Unsupported {
        content_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionState {
    Pending,
    Resolved,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionAction {
    Submit,
    Cancel,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Attachment {
    pub kind: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptionReason {
    Canceled,
    Failed,
    BackendUnavailable,
}
