use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{MessageId, SubagentId, TaskId};

use super::{AgentPlanSnapshot, ChatSnapshot};

/// Lightweight Task-snapshot projection. Complete hierarchy and history use
/// independent subscription scopes so ordinary Task opens stay bounded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubagentOverviewSnapshot {
    pub total_count: u64,
    pub running_count: u64,
    pub attention_count: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SubagentStatus {
    WaitingForActivity,
    Running,
    Completed,
    Failed,
    Cancelled,
    Disconnected,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCapabilitiesSnapshot {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cancel: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDetailSnapshot {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogEntrySnapshot {
    pub subagent_id: SubagentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_subagent_id: Option<SubagentId>,
    pub name: String,
    pub delegated_task: String,
    pub status: SubagentStatus,
    pub capabilities: SubagentCapabilitiesSnapshot,
    pub spawned_order: u64,
    pub history_revision: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub history_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<SubagentDetailSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogSnapshot {
    pub task_id: TaskId,
    pub revision: u64,
    pub entries: Vec<SubagentCatalogEntrySnapshot>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SubagentHistoryAvailability {
    Available,
    WaitingForActivity,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubagentHistorySnapshot {
    pub task_id: TaskId,
    pub subagent_id: SubagentId,
    pub revision: u64,
    pub availability: SubagentHistoryAvailability,
    pub chat: ChatSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_plan: Option<AgentPlanSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_cursor: Option<MessageId>,
}
