use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use openaide_app_server_protocol::server_requests::{QuestionField, QuestionValue};

use super::{
    ActivityStatus, ActivityStep, PermissionDecision, PermissionOption, PermissionState,
    PermissionToolCall,
};

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
    Permission {
        id: String,
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        app_server_request_id: Option<String>,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        risk: Option<String>,
        tool_call: PermissionToolCall,
        state: PermissionState,
        created_at: String,
        options: Vec<PermissionOption>,
        #[serde(skip_serializing_if = "Option::is_none")]
        selected_option: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision: Option<PermissionDecision>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resolution_message: Option<String>,
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
            NormalizedMessage::Permission { .. } => "permission",
            NormalizedMessage::Question { .. } => "question",
            NormalizedMessage::Interruption { .. } => "interruption",
        }
    }

    pub fn identity(&self) -> String {
        match self {
            NormalizedMessage::User { id, .. }
            | NormalizedMessage::AgentMessage { id, .. }
            | NormalizedMessage::Activity { id, .. }
            | NormalizedMessage::Permission { id, .. }
            | NormalizedMessage::Question { id, .. }
            | NormalizedMessage::Interruption { id, .. } => id.clone(),
        }
    }

    pub fn preserve_created_at_from(&mut self, existing: &NormalizedMessage) {
        let existing_created_at = match existing {
            NormalizedMessage::User { created_at, .. }
            | NormalizedMessage::AgentMessage { created_at, .. }
            | NormalizedMessage::Activity { created_at, .. }
            | NormalizedMessage::Permission { created_at, .. }
            | NormalizedMessage::Question { created_at, .. }
            | NormalizedMessage::Interruption { created_at, .. } => created_at.clone(),
        };
        match self {
            NormalizedMessage::User { created_at, .. }
            | NormalizedMessage::AgentMessage { created_at, .. }
            | NormalizedMessage::Activity { created_at, .. }
            | NormalizedMessage::Permission { created_at, .. }
            | NormalizedMessage::Question { created_at, .. }
            | NormalizedMessage::Interruption { created_at, .. } => {
                *created_at = existing_created_at
            }
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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
