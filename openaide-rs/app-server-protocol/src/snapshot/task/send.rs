use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSendCapabilitySnapshot {
    pub state: TaskSendCapabilityState,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attachment_only: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<TaskSendBlocker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskSendCapabilityState {
    Loading,
    Ready,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSendBlocker {
    pub kind: TaskSendBlockerKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskSendBlockerKind {
    TaskPreparing,
    TaskRunning,
    AgentConfigNotReady,
    SlashCommandsNotReady,
    AttachmentsNeedRefresh,
    EmptyMessage,
    MissingRequiredOptions,
    FailedValidation,
}
