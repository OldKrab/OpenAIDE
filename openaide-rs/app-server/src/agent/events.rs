use crate::protocol::model::{
    ActivityToolDetails, AgentCommandsCatalog, AgentMessagePart, AgentMessageRole,
    ConfigOptionsCatalog,
};

#[derive(Debug, Clone)]
pub enum AgentEvent {
    MessageChunk {
        role: AgentMessageRole,
        part: AgentMessagePart,
        /// ACP correlation key shared by every ordered part of one message.
        source_message_id: Option<String>,
    },
    ToolCall(AgentToolCall),
    /// One ordered ACP Tool update. Summary and terminal changes share this
    /// envelope so a mixed wire update cannot be reordered at the event seam.
    ToolUpdate(AgentToolUpdate),
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
pub struct AgentToolUpdate {
    /// Present only when lightweight or structured Tool projection changed.
    pub summary: Option<AgentToolCall>,
    /// Agent-owned terminal appends in wire arrival order.
    pub terminal_appends: Vec<AgentTerminalAppend>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AgentTerminalAppend {
    pub tool_call_id: String,
    pub terminal_id: String,
    pub data: String,
}

impl std::fmt::Debug for AgentTerminalAppend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AgentTerminalAppend")
            .field("tool_call_id", &self.tool_call_id)
            .field("terminal_id_bytes", &self.terminal_id.len())
            .field("data_bytes", &self.data.len())
            .finish()
    }
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
