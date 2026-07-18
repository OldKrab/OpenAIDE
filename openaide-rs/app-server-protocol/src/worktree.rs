use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ProjectId, TaskId, WorktreeId, WorktreeOperationId, WorktreeRepositoryId};

/// Authoritative repository-scoped projection consumed by selectors and management.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRepositorySnapshot {
    pub repository_id: WorktreeRepositoryId,
    pub revision: u64,
    pub worktrees: Vec<WorktreeSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bases: Vec<WorktreeBaseSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<WorktreeOperationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRefreshParams {
    pub project_id: ProjectId,
    pub repository_id: WorktreeRepositoryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRefreshResult {
    pub repository: WorktreeRepositorySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WorktreeBaseSelection {
    CurrentHead,
    LocalBranch { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateParams {
    pub project_id: ProjectId,
    pub repository_id: WorktreeRepositoryId,
    pub name: String,
    pub base: WorktreeBaseSelection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateResult {
    pub operation_id: WorktreeOperationId,
    pub repository: WorktreeRepositorySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRecreateParams {
    pub project_id: ProjectId,
    pub repository_id: WorktreeRepositoryId,
    pub worktree_id: WorktreeId,
    pub base: WorktreeBaseSelection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRecreateResult {
    pub operation_id: WorktreeOperationId,
    pub repository: WorktreeRepositorySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemovalPreflightParams {
    pub repository_id: WorktreeRepositoryId,
    pub worktree_id: WorktreeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemovalPreflightResult {
    pub preflight: WorktreeRemovalPreflight,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoveParams {
    pub repository_id: WorktreeRepositoryId,
    pub worktree_id: WorktreeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoveResult {
    pub operation_id: WorktreeOperationId,
    pub repository: WorktreeRepositorySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRenameParams {
    pub repository_id: WorktreeRepositoryId,
    pub worktree_id: WorktreeId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRenameResult {
    pub repository: WorktreeRepositorySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeResolveFolderParams {
    pub repository_id: WorktreeRepositoryId,
    pub worktree_id: WorktreeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeResolveFolderResult {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSummary {
    pub worktree_id: WorktreeId,
    pub name: String,
    pub path: String,
    /// Historical metadata retained for linked Task history after the worktree is removed.
    #[serde(default)]
    pub forgotten: bool,
    pub ownership: WorktreeOwnership,
    pub is_main: bool,
    pub head: WorktreeHead,
    pub availability: WorktreeAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prunable_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_ids: Vec<ProjectId>,
    pub linked_task_count: u32,
    pub running_task_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorktreeOwnership {
    Managed,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorktreeAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WorktreeHead {
    Branch { name: String, commit: String },
    Detached { commit: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WorktreeBaseSnapshot {
    Head { commit: String, label: String },
    LocalBranch { name: String, commit: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorktreeOperationKind {
    Create,
    Recreate,
    Remove,
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorktreeOperationState {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeOperationSnapshot {
    pub operation_id: WorktreeOperationId,
    pub kind: WorktreeOperationKind,
    pub state: WorktreeOperationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_files: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_files: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeLinkedTasksParams {
    pub repository_id: WorktreeRepositoryId,
    pub worktree_id: WorktreeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeLinkedTasksResult {
    pub task_ids: Vec<TaskId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorktreeRemovalStatus {
    Safe,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorktreeRemovalBlocker {
    PrimaryWorktree,
    RunningTasks,
    Locked,
    Unavailable,
    WorkingTreeChanges,
    DetachedCommits,
    InitializedSubmodules,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemovalPreflight {
    pub status: WorktreeRemovalStatus,
    pub blockers: Vec<WorktreeRemovalBlocker>,
    pub ownership: WorktreeOwnership,
    pub path: String,
    pub ignored_files_will_be_removed: bool,
}
