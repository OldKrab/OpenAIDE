use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AgentId, TaskId};
use crate::snapshot::TaskStatus;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiagnosticsParams {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiagnosticsResult {
    pub status: RuntimeDiagnosticsStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub method_count: usize,
    pub tasks: TaskDiagnosticsResult,
    pub redaction: DiagnosticsRedaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeDiagnosticsStatus {
    Ready,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskDiagnosticsResult {
    pub visible_count: usize,
    pub total_count: usize,
    pub active_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_tasks: Vec<ActiveTaskDiagnosticsResult>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTaskDiagnosticsResult {
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub status: TaskStatus,
    pub updated_at: String,
    pub last_activity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_id: Option<String>,
    pub has_agent_session: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticsRedaction {
    PromptTextFileContentsTerminalOutputAndSecretsRemoved,
}
