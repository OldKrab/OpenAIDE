use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::client::{RequestedSurface, ShellKind};
use crate::ids::{AgentId, ClientInstanceId, EventCursor, ProjectId, ServerId, StateRootId};

pub(crate) mod chat;
pub(crate) mod pending_request;
pub(crate) mod settings;
pub(crate) mod task;

pub use chat::*;
pub use pending_request::*;
pub use settings::*;
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
    #[serde(default, skip_serializing_if = "AgentCapabilities::is_empty")]
    pub capabilities: AgentCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    Disconnected,
    Launching,
    Connected,
    SetupRequired,
    AuthRequired,
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
}

impl AgentCapabilities {
    pub fn is_empty(&self) -> bool {
        !self.resume_tasks && !self.delete_native_sessions
    }
}

#[cfg(test)]
mod tests;
