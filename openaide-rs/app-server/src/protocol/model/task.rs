use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::storage::records::{TaskLifecycle, TaskPreparationRecord};

use super::{AgentCommandsCatalog, ConfigOptionsCatalog, MessagePage, NormalizedMessage};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Active,
    Inactive,
    Failed,
    Completed,
    Blocked,
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
    pub title: String,
    pub status: TaskStatus,
    pub task_version: u64,
    pub message_history_version: u64,
    pub unread: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity: String,
    pub agent_id: String,
    pub agent_name: String,
    pub isolation: IsolationKind,
    pub workspace_root: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskSnapshot {
    pub task: TaskSummary,
    pub lifecycle: TaskLifecycle,
    pub chat: MessagePage,
    pub permissions: Vec<NormalizedMessage>,
    pub settings_summary: SettingsSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_options_catalog: Option<ConfigOptionsCatalog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_config_change: Option<PendingTaskConfigChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_commands_catalog: Option<AgentCommandsCatalog>,
    pub preparation: TaskPreparationRecord,
    pub revision: u64,
}

/// Process-neutral projection of an in-flight config mutation.
/// Server sequencing remains private to the durable Task record.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PendingTaskConfigChange {
    pub client_mutation_id: String,
    pub config_id: String,
    pub requested_value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SettingsSummary {
    pub agent_id: String,
    pub isolation: IsolationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config_options: HashMap<String, String>,
}
