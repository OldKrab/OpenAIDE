use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::client::{RequestedSurface, ShellKind};
use crate::ids::{
    AgentId, ClientInstanceId, EventCursor, ProjectId, ServerId, StateRootId, WorktreeId,
    WorktreeRepositoryId,
};

pub(crate) mod chat;
pub(crate) mod pending_request;
pub(crate) mod settings;
pub(crate) mod subagent;
pub(crate) mod task;

pub use chat::*;
pub use pending_request::*;
pub use settings::*;
pub use subagent::*;
pub use task::*;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientSnapshot {
    pub cursor: EventCursor,
    pub server: ServerSnapshot,
    pub state_root: StateRootSnapshot,
    pub client: ClientSnapshotScope,
    pub new_task_defaults: NewTaskDefaultsSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<ProjectCollectionSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<AgentCollectionSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<TaskNavigationSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task: Option<TaskSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<SettingsSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_requests: Vec<PendingRequestSnapshot>,
}

/// State-root-wide initial selection for a client that has no retained New Task choice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NewTaskDefaultsSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ServerSnapshot {
    pub server_id: ServerId,
    pub protocol_version: ProtocolVersion,
    #[serde(default, skip_serializing_if = "ServerCapabilities::is_empty")]
    pub capabilities: ServerCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reconnect: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resync: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub streaming_events: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub frontend_requests: bool,
}

impl ServerCapabilities {
    pub fn is_empty(&self) -> bool {
        !self.reconnect && !self.resync && !self.streaming_events && !self.frontend_requests
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StateRootSnapshot {
    pub state_root_id: StateRootId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientSnapshotScope {
    pub client_instance_id: ClientInstanceId,
    pub shell_kind: ShellKind,
    pub surface: RequestedSurface,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCollectionSnapshot {
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub project_id: ProjectId,
    pub label: String,
    pub workspace_root: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_repository_id: Option<WorktreeRepositoryId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_worktree_id: Option<WorktreeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentCollectionSnapshot {
    pub agents: Vec<AgentSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSummary {
    pub agent_id: AgentId,
    pub label: String,
    pub status: AgentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_reason: Option<AgentSetupReason>,
    #[serde(default, skip_serializing_if = "AgentCapabilities::is_empty")]
    pub capabilities: AgentCapabilities,
    /// The one Sign-in Flow App Server is running (or last ran without success) for this Agent.
    /// Absent when no flow is running and the last flow ended in success or cancellation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<AgentSignInFlow>,
}

/// App Server-owned state of an Agent Sign-in Flow. Every connected client observes the same
/// flow, including the verification URL and hint, so a reloaded tab or second device can finish
/// a device-code login that another tab started.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentSignInFlow {
    /// Agent-advertised Authentication Method id the user chose.
    pub method_id: String,
    pub phase: AgentSignInPhase,
    /// Verification URL supplied by the Agent while `awaitingUser`. Always HTTPS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Agent-supplied instructions shown next to the URL, such as a one-time device code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Product-safe failure summary while `failed`. Never carries Agent error text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentSignInPhase {
    /// The method was accepted and the Agent has not asked the user for anything yet.
    Starting,
    /// The Agent asked the user to open a URL (and possibly enter a hint such as a device code).
    AwaitingUser,
    /// A terminal-kind method opened a terminal; the user must confirm when it finishes.
    AwaitingTerminal,
    /// The flow ended without success. The user dismisses it by starting another flow or
    /// cancelling.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentSetupReason {
    NodeJsRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    Disconnected,
    Installing,
    Launching,
    Connected,
    SetupRequired,
    AuthRequired,
    Authenticating,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resume_tasks: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub delete_native_sessions: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fork_native_sessions: bool,
}

impl AgentCapabilities {
    pub fn is_empty(&self) -> bool {
        !self.resume_tasks && !self.delete_native_sessions && !self.fork_native_sessions
    }
}

#[cfg(test)]
mod tests;
