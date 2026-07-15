use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AgentId, ProjectId, TaskId};

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
    pub tasks: Vec<TaskSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task_id: Option<TaskId>,
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
    pub lifecycle: TaskLifecycle,
    pub revision: u64,
    pub preparation: TaskPreparationSnapshot,
    pub agent_config: TaskAgentConfigSnapshot,
    pub agent_commands: TaskAgentCommandsSnapshot,
    pub send_capability: TaskSendCapabilitySnapshot,
    pub chat: ChatSnapshot,
    pub history_sync: TaskHistorySyncSnapshot,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_requests: Vec<PendingRequestSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoverySnapshot>,
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
