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

    /// A client is compatible when it shares this server's major version and
    /// was built against a minor version this server already implements.
    pub fn accepts_client(self, client: Self) -> bool {
        self.major == client.major && client.minor <= self.minor
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

impl std::str::FromStr for ProtocolVersion {
    type Err = ProtocolVersionParseError;

    /// A bare major such as `2` is the form written before minors existed.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (major, minor) = value.split_once('.').unwrap_or((value, "0"));
        Ok(Self {
            major: major.parse().map_err(|_| ProtocolVersionParseError)?,
            minor: minor.parse().map_err(|_| ProtocolVersionParseError)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersionParseError;

impl std::fmt::Display for ProtocolVersionParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("protocol version must be major.minor")
    }
}

impl std::error::Error for ProtocolVersionParseError {}

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
    /// Configured display icon id for this Agent. Unknown ids stay opaque here and
    /// are normalized by the Frontend, which owns icon rendering.
    pub icon: String,
    pub status: AgentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_reason: Option<AgentSetupReason>,
    #[serde(default, skip_serializing_if = "AgentCapabilities::is_empty")]
    pub capabilities: AgentCapabilities,
    /// The one Sign-in Flow App Server is running (or last ran without success) for this Agent.
    /// Absent when no flow is running and the last flow ended in success or cancellation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<AgentSignInFlow>,
    /// Usage windows of the account this Agent is signed in to. Absent until the Agent reports
    /// them; every Task of the Agent shares the same value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_limits: Option<AgentAccountLimits>,
}

/// Subscription usage limits an Agent reports for its signed-in account. They belong to the
/// account, not to a Task or Native Session, and are held in memory only: a restarted App Server
/// has none until the Agent reports again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccountLimits {
    /// Display name of the subscription plan when the Agent reports one, for example "Max".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_label: Option<String>,
    /// Ordered for display: the 5-hour window, the weekly window, then per-model weekly windows.
    pub windows: Vec<AgentAccountLimitWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccountLimitWindow {
    pub kind: AgentAccountLimitWindowKind,
    /// Only a `weeklyModel` window carries the model family it meters, for example "Opus".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_label: Option<String>,
    /// Share of the window already used, 0 to 100.
    pub used_percent: u8,
    /// Unix epoch milliseconds at which the window resets. Absent when the Agent did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at_ms: Option<u64>,
    pub status: AgentAccountLimitStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentAccountLimitWindowKind {
    FiveHour,
    Weekly,
    WeeklyModel,
}

/// The Agent's own verdict for a window; App Server never derives a warning from a percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentAccountLimitStatus {
    Ok,
    Warning,
    Reached,
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
