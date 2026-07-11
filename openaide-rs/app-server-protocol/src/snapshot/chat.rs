use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AttachmentId, MessageId, RequestId, TurnId};
use crate::server_requests::{PermissionToolCallRef, QuestionField, QuestionValue};
use std::collections::BTreeMap;
use crate::task::TaskToolDetailResult;

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
    Activity {
        title: String,
        status: ActivityStatus,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        steps: Vec<ActivityStepSnapshot>,
    },
    Permission {
        request_id: RequestId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_server_request_id: Option<RequestId>,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        risk: Option<String>,
        tool_call: PermissionToolCallRef,
        state: PermissionMessageState,
        options: Vec<PermissionMessageOption>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected_option: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision: Option<PermissionMessageDecision>,
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PermissionMessageOption {
    pub option_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PermissionMessageOptionKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMessageOptionKind {
    Allow,
    Deny,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMessageState {
    Pending,
    Responding,
    Resolved,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMessageDecision {
    Approved,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ActivityStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
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
        input_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_preview: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail_artifact_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<TaskToolDetailResult>,
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
