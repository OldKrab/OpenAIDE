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
    pub title: String,
    pub status: TaskStatus,
    pub updated_at: String,
    pub last_activity: String,
    pub unread: bool,
    pub has_messages: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Preparing,
    Idle,
    Running,
    Blocked,
    Interrupted,
    Failed,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub task: TaskSummary,
    pub revision: u64,
    pub preparation: TaskPreparationSnapshot,
    pub agent_config: TaskAgentConfigSnapshot,
    pub agent_commands: TaskAgentCommandsSnapshot,
    pub send_capability: TaskSendCapabilitySnapshot,
    pub chat: ChatSnapshot,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_requests: Vec<PendingRequestSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoverySnapshot>,
}
