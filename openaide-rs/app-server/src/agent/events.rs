use crate::protocol::model::{ActivityToolDetails, AgentCommandsCatalog, ConfigOptionsCatalog};

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Text(String),
    Thought(String),
    TextChunk {
        text: String,
        /// Agent correlation key used only to preserve streamed message boundaries.
        source_message_id: Option<String>,
    },
    ThoughtChunk {
        text: String,
        /// Agent correlation key used only to preserve streamed message boundaries.
        source_message_id: Option<String>,
    },
    ToolCall(AgentToolCall),
    Activity {
        title: String,
        tool_name: String,
        output_preview: String,
    },
    PermissionRequest(AgentPermissionRequest),
    ConfigOptionsChanged(ConfigOptionsCatalog),
    CommandsChanged(AgentCommandsCatalog),
}

#[derive(Debug, Clone)]
pub struct AgentToolCall {
    pub tool_call_id: String,
    pub scope_id: Option<String>,
    pub title: String,
    pub kind: String,
    pub status: AgentToolCallStatus,
    pub input_summary: Option<String>,
    pub output_preview: Option<String>,
    pub details: Option<Box<ActivityToolDetails>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AgentPermissionRequest {
    pub request_id: String,
    pub title: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub risk: Option<String>,
    pub tool_call: AgentToolCallRef,
    pub options: Vec<AgentPermissionOption>,
}

#[derive(Debug, Clone)]
pub struct AgentToolCallRef {
    pub tool_call_id: String,
    pub title: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentPermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: AgentPermissionOptionKind,
}

#[derive(Debug, Clone, Copy)]
pub enum AgentPermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

#[derive(Debug, Clone)]
pub enum AgentPermissionOutcome {
    Selected { option_id: String },
    Cancelled,
}
