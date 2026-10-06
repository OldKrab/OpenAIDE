use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::task::AgentPlanEntrySnapshot;
use crate::ids::{AttachmentId, MessageId, RequestId, SubagentId, TurnId};
use crate::server_requests::{QuestionField, QuestionValue};
use crate::task::ToolDetailSnapshot;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshot {
    pub items: Vec<ChatItem>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_more_before: bool,
    pub has_messages: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_cursor: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_cursor: Option<MessageId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ChatItem {
    pub message_id: MessageId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    pub role: ChatRole,
    pub status: ChatItemStatus,
    pub parts: Vec<MessagePart>,
    /// Times the App Server observed for this row while it was live. Absent for
    /// history it did not witness, such as a reloaded native session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<ChatItemTiming>,
}

/// App Server-clock facts about one Chat row. Every field is optional because
/// each is recorded only when the App Server witnessed the moment itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ChatItemTiming {
    /// When the user's message was accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    /// How long this row's own work ran: one Activity or one compaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<TimeSpanSnapshot>,
    /// The turn this row closed: set on the turn's final Agent answer, or on the
    /// interruption that ended it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_turn: Option<TimeSpanSnapshot>,
}

/// Epoch-millisecond bounds on the App Server clock. `ended_at` is absent while
/// the work is still running.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TimeSpanSnapshot {
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ChatRole {
    User,
    Agent,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ChatItemStatus {
    Complete,
    Streaming,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MessagePart {
    Text {
        text: String,
    },
    Attachment {
        attachment: AttachmentSnapshot,
    },
    Image {
        media_type: String,
        data_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
    },
    Resource {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size_bytes: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Unsupported {
        content_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
    },
    Activity {
        title: String,
        status: ActivityStatus,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        steps: Vec<ActivityStepSnapshot>,
    },
    Question {
        request_id: RequestId,
        message: String,
        fields: Vec<QuestionField>,
        state: QuestionMessageState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<QuestionMessageAction>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<BTreeMap<String, QuestionValue>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolution_message: Option<String>,
    },
    CompletedPlan {
        entries: Vec<AgentPlanEntrySnapshot>,
    },
    ClosedPlan {
        entries: Vec<AgentPlanEntrySnapshot>,
    },
    /// An Agent-owned context compaction. `summary` is user-displayable
    /// Markdown supplied by the Agent, absent until or unless it provides one.
    Compaction {
        status: CompactionStatusSnapshot,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CompactionStatusSnapshot {
    InProgress,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum QuestionMessageState {
    Pending,
    Resolved,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum QuestionMessageAction {
    Submit,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ActivityStatus {
    Running,
    Completed,
    Interrupted,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SubagentActivitySnapshot {
    Delegated,
    Interacted,
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolPresentationSnapshot {
    pub actions: Vec<ToolPresentationActionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolPresentationActionSnapshot {
    Skill {
        subjects: Vec<String>,
    },
    Read {
        subjects: Vec<String>,
    },
    View {
        subjects: Vec<String>,
    },
    List {
        subjects: Vec<String>,
    },
    Search {
        query: String,
        scopes: Vec<String>,
        target: ToolSearchTargetSnapshot,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ToolSearchTargetSnapshot {
    Contents,
    Paths,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
// Snapshot variants mirror the serialized contract; boxing only the Rust side
// would add protocol-boundary ownership complexity without changing the wire.
#[allow(clippy::large_enum_variant)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ActivityStepSnapshot {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<String>,
    },
    Tool {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        name: String,
        status: ActivityStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<ToolPresentationSnapshot>,
        /// Agent-authored purpose of an execute Tool, shown as its compact title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_preview: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail_artifact_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<ToolDetailSnapshot>,
        permission_outcomes: Vec<ToolPermissionOutcomeSnapshot>,
    },
    Command {
        command_label: String,
        status: ActivityStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_preview: Option<String>,
    },
    Subagent {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subagent_id: Option<SubagentId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity: Option<String>,
        name: String,
        path: Vec<String>,
        status: ActivityStatus,
        events: Vec<SubagentActivitySnapshot>,
    },
}

/// One durable App Server permission decision projected inside its linked tool.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolPermissionOutcomeSnapshot {
    pub request_id: RequestId,
    pub decision: ToolPermissionDecisionSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_label: Option<String>,
    pub resolved_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ToolPermissionDecisionSnapshot {
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentSnapshot {
    pub attachment_id: AttachmentId,
    pub kind: AttachmentKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKind {
    FileReference,
    EmbeddedSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySnapshot {
    pub message: String,
    pub actions: Vec<RecoveryAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryAction {
    Continue,
    ReuseLastPrompt,
}
