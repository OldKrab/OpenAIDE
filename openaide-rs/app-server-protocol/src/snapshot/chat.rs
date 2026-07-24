use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AttachmentId, MessageId, RequestId, TurnId};
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolPresentationSnapshot {
    pub kind: ToolPresentationKindSnapshot,
    pub subjects: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ToolPresentationKindSnapshot {
    Skill,
    Read,
    List,
    Search,
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
