use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AgentId, ProjectId, TaskId, WorktreeId};

use super::chat::{ChatSnapshot, RecoverySnapshot};
use super::pending_request::PendingRequestSnapshot;

mod agent;
mod preparation;
mod send;

pub use agent::*;
pub use preparation::*;
pub use send::*;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskNavigationSnapshot {
    /// Combined authoritative Navigation rows.
    #[serde(default)]
    pub entries: Vec<TaskNavigationEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refreshing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskNavigationEntry {
    Task { task: Box<TaskSummary> },
    NativeSession { session: NativeSessionSummary },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionSummary {
    pub reference: NativeSessionReference,
    pub project_id: ProjectId,
    pub workspace_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionReference {
    pub agent_id: AgentId,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub task_id: TaskId,
    pub project_id: ProjectId,
    pub agent_id: AgentId,
    pub title: Option<TaskTitle>,
    pub status: TaskStatus,
    pub updated_at: String,
    pub last_activity: String,
    pub unread: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<TaskAttentionEvent>,
    pub has_messages: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
    /// Availability is independent of Task runtime status so history remains readable.
    pub workspace_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttentionEvent {
    pub event_id: String,
    pub reason: TaskAttentionReason,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskAttentionReason {
    Finished,
    NeedsPermission,
    NeedsAnswer,
    Stopped,
    Failed,
}

/// A Task title together with the authority that most recently set it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskTitle {
    pub value: String,
    pub source: TaskTitleSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskTitleSource {
    Prompt,
    Agent,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Preparing,
    Starting,
    Idle,
    Running,
    Stopping,
    Waiting,
    Interrupted,
    Failed,
    Completed,
}

/// Client-visible Task history membership. New Task ownership remains private to App Server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskLifecycle {
    New,
    Visible,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub task: TaskSummary,
    /// App Server-authored start of the active turn; absent when no turn is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_started_at: Option<String>,
    pub lifecycle: TaskLifecycle,
    pub revision: u64,
    pub preparation: TaskPreparationSnapshot,
    pub agent_config: TaskAgentConfigSnapshot,
    pub agent_commands: TaskAgentCommandsSnapshot,
    pub send_capability: TaskSendCapabilitySnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_capabilities: Option<TaskInputCapabilities>,
    pub chat: ChatSnapshot,
    pub history_sync: TaskHistorySyncSnapshot,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_requests: Vec<PendingRequestSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoverySnapshot>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskInputCapabilities {
    pub image: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskHistorySyncSnapshot {
    Idle { generation: u64 },
    Syncing { generation: u64 },
    Updated { generation: u64 },
}

impl Default for TaskHistorySyncSnapshot {
    fn default() -> Self {
        Self::Idle { generation: 0 }
    }
}
