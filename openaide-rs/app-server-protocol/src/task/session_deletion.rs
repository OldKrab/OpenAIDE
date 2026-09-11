use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AgentId, ProjectId, TaskId};
use crate::snapshot::NativeSessionReference;

/// One Agent-owned identity, whether or not OpenAIDE has adopted it as a Task.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(rename_all = "camelCase")]
pub enum NativeSessionDeleteTarget {
    Task {
        task_id: TaskId,
    },
    NativeSession {
        agent_id: AgentId,
        native_session_id: String,
    },
}

/// Confirmation is checked against current activity and queued work at dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionDeleteConfirmation {
    pub active: bool,
    pub queued_message_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionDeleteParams {
    pub target: NativeSessionDeleteTarget,
    /// Omit to obtain the authoritative confirmation details without deleting anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<NativeSessionDeleteConfirmation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(rename_all = "camelCase")]
pub enum NativeSessionDeleteResult {
    ConfirmationRequired {
        title: String,
        active: bool,
        queued_message_count: usize,
    },
    Deleted {
        reference: NativeSessionReference,
        project_id: ProjectId,
        task_id: Option<TaskId>,
    },
}
