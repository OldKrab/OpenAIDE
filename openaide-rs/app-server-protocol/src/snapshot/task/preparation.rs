use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::ProtocolError;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskPreparationSnapshot {
    Preparing {
        steps: Vec<TaskPreparationStep>,
    },
    Ready,
    Blocked {
        blocker: TaskSetupBlocker,
        actions: Vec<TaskPreparationAction>,
    },
    Failed {
        error: ProtocolError,
        actions: Vec<TaskPreparationAction>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskPreparationStep {
    pub kind: TaskPreparationStepKind,
    pub status: TaskPreparationStepStatus,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskPreparationStepKind {
    CreatingTask,
    ResolvingProject,
    ResolvingMcpServers,
    StartingAgent,
    CreatingNativeSession,
    CheckingAuthentication,
    LoadingConfigOptions,
    LoadingSlashCommands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskPreparationStepStatus {
    Pending,
    Running,
    Done,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetupBlocker {
    pub kind: TaskSetupBlockerKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskSetupBlockerKind {
    AuthRequired,
    SetupRequired,
    NodeJsRequired,
    CapabilityUnavailable,
    NativeSessionUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskPreparationAction {
    Retry,
    ChangeAgent,
    Discard,
    OpenAgentSettings,
    Authenticate,
}
