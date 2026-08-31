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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportExportListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SupportArtifactAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportExportSession {
    pub task_id: TaskId,
    pub title: String,
    pub agent_id: AgentId,
    pub agent_name: String,
    pub project_label: String,
    pub last_activity: String,
    pub active: bool,
    pub acp_trace_count: usize,
    pub native_transcript: SupportArtifactAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportExportTrace {
    pub trace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    pub operation: String,
    pub modified_at: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportExportListResult {
    pub sessions: Vec<SupportExportSession>,
    pub unbound_traces: Vec<SupportExportTrace>,
    pub acp_trace_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportExportSessionSelection {
    pub task_id: TaskId,
    pub include_openaide_history: bool,
    pub include_acp_traces: bool,
    pub include_native_transcript: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportExportCreateParams {
    pub include_runtime_snapshot: bool,
    pub include_logs: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<SupportExportSessionSelection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unbound_trace_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportExportCreateResult {
    /// Client-bound opaque handle; local paths never cross the product protocol.
    pub file_handle_id: String,
    pub label: String,
    pub size_bytes: u64,
    pub contains_sensitive_data: bool,
}
