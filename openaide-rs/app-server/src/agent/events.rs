use crate::protocol::model::{
    ActivityStatus, ActivityToolDetails, AgentCommandsCatalog, AgentMessagePart, AgentMessageRole,
    AgentPlan, ConfigOptionsCatalog, ToolPresentation,
};

#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// User-authored input reported on a native child session stream.
    UserMessageChunk {
        text: String,
        source_message_id: Option<String>,
    },
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
    /// Protocol-faithful projection of one Codex subagent Tool call.
    Subagent(AgentSubagent),
    Activity {
        title: String,
        tool_name: String,
        output_preview: String,
    },
    PermissionRequest(AgentPermissionRequest),
    ConfigOptionsChanged(ConfigOptionsCatalog),
    CommandsChanged(AgentCommandsCatalog),
    Plan(AgentPlan),
    ContextUsage(AgentContextUsage),
    TurnUsage(AgentTurnUsage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSubagent {
    /// Tool identity keeps start and interaction calls as separate transcript rows.
    pub tool_call_id: String,
    /// Agent-owned ACP title; the product must not replace it with inferred lifecycle copy.
    pub title: String,
    pub thread_id: String,
    pub path: String,
    pub activity: String,
    pub status: ActivityStatus,
}

/// Provider-neutral lifecycle announcement for a negotiated ACP child session.
/// Native session identifiers remain inside the Agent/App Server boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNativeSubagentSpawned {
    pub parent_native_session_id: String,
    pub native_session_id: String,
    pub name: String,
    /// `None` means the provider announced the child but did not expose the delegated prompt.
    pub delegated_task: Option<String>,
    /// The provider uses a repeated announcement to signal a parent-to-child interaction.
    /// This stays typed at the Agent boundary instead of leaking provider metadata downstream.
    pub parent_interaction: bool,
    pub capabilities: AgentNativeSubagentCapabilities,
    pub details: Vec<AgentNativeSubagentDetail>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentNativeSubagentCapabilities {
    pub cancel: bool,
    pub close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNativeSubagentDetail {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentNativeSubagentState {
    Completed,
    Failed,
    Cancelled,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNativeSubagentStateUpdate {
    pub parent_native_session_id: String,
    pub native_session_id: String,
    pub state: AgentNativeSubagentState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextUsage {
    pub used_tokens: u64,
    pub capacity_tokens: u64,
    pub cost: Option<AgentUsageCost>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentUsageCost {
    /// Text preserves the ACP decimal value without introducing float equality into snapshots.
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTurnUsage {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: Option<u64>,
    pub cached_read_tokens: Option<u64>,
    pub cached_write_tokens: Option<u64>,
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
    pub presentation: Option<ToolPresentation>,
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
    /// Present only when a task-wide request originated from a native child session.
    /// This stays inside the Agent/App Server boundary and routes durable attribution.
    pub subagent_native_session_id: Option<String>,
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
