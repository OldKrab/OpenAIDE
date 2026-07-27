use crate::storage::records::{
    TaskAttentionEvent, TaskLifecycle, TaskPreparationRecord, TaskTitle,
};
use serde::{Deserialize, Serialize};

use super::{AgentCommandsCatalog, ConfigOptionCurrentValue, ConfigOptionsCatalog, MessagePage};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Starting,
    Active,
    Stopping,
    Inactive,
    Failed,
    Completed,
    Waiting,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IsolationKind {
    Local,
    GitWorktree,
    Docker,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskSummary {
    pub task_id: String,
    pub title: Option<TaskTitle>,
    pub status: TaskStatus,
    pub task_version: u64,
    pub message_history_version: u64,
    pub unread: bool,
    pub attention: Option<TaskAttentionEvent>,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity: String,
    pub agent_id: String,
    pub agent_name: String,
    pub isolation: IsolationKind,
    pub workspace_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskSnapshot {
    pub task: TaskSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_turn_started_at: Option<String>,
    pub lifecycle: TaskLifecycle,
    pub chat: MessagePage,
    pub settings_summary: SettingsSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_options_catalog: Option<ConfigOptionsCatalog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_config_change: Option<PendingTaskConfigChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_commands_catalog: Option<AgentCommandsCatalog>,
    pub preparation: TaskPreparationRecord,
    pub supports_image_input: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_usage: Option<TaskContextUsage>,
    pub revision: u64,
}

/// Agent-reported usage for the live Native Session bound to this Task.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskContextUsage {
    pub used_tokens: u64,
    pub capacity_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<TaskUsageCost>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_turn: Option<TaskTurnUsage>,
}

/// Currency amount is stored as text so protocol state remains exact and Eq-safe.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskUsageCost {
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskTurnUsage {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_read_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_write_tokens: Option<u64>,
}

/// Process-neutral projection of an in-flight config mutation.
/// Server sequencing remains private to the durable Task record.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PendingTaskConfigChange {
    pub client_mutation_id: String,
    pub config_id: String,
    pub requested_value: ConfigOptionCurrentValue,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SettingsSummary {
    pub agent_id: String,
    pub isolation: IsolationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}
