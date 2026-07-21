use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{
    AgentConfigOptionId, AgentId, AttachmentHandleId, ClientMutationId, MessageId, ProjectId,
    TaskId, TaskListCursor, TurnId, WorktreeId,
};
use crate::snapshot::AgentConfigOptionCurrentValue;
use crate::snapshot::{ChatItem, TaskSnapshot, TaskSummary};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAcquireParams {
    pub project_id: ProjectId,
    pub agent_id: AgentId,
    /// Legacy bootstrap fallback for Projects not yet present in the App Server catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
}

/// Acquires from the same prepared pool as `task/acquire`, resolving an opaque Worktree first.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAcquireInWorktreeParams {
    pub project_id: ProjectId,
    pub agent_id: AgentId,
    pub worktree_id: WorktreeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAcquireInWorktreeResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAcquireResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSearchFilesParams {
    pub task_id: TaskId,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSearchFilesResult {
    pub task_id: TaskId,
    pub state: WorkspaceFileSearchState,
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceFileSearchState {
    Ready,
    Refreshing,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAdoptNativeSessionParams {
    pub project_id: ProjectId,
    pub agent_id: AgentId,
    pub native_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAdoptNativeSessionResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSendParams {
    pub task_id: TaskId,
    pub message: ComposerMessage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComposerMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ComposerImage>,
    /// Ordered App Server-owned resources selected before Send.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentHandleId>,
}

/// One Frontend-owned Image encoded only as part of the Send mutation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComposerImage {
    pub label: String,
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSendResult {
    pub task: TaskSnapshot,
    pub turn_id: TurnId,
    pub user_message_id: MessageId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetConfigOptionParams {
    pub task_id: TaskId,
    pub config_id: AgentConfigOptionId,
    pub value: AgentConfigOptionCurrentValue,
    pub client_mutation_id: ClientMutationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetConfigOptionResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskCancelParams {
    pub task_id: TaskId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskCancelResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskOpenParams {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskOpenResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskMarkReadParams {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskMarkReadResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskChatPageParams {
    pub task_id: TaskId,
    pub before_cursor: MessageId,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskChatPageResult {
    pub task_id: TaskId,
    pub items: Vec<ChatItem>,
    pub has_before: bool,
    pub total_count: u64,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_cursor: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_cursor: Option<MessageId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolDetailSnapshot {
    /// Artifact-local durable revision used to reject a delta already covered by a baseline.
    #[serde(default)]
    pub revision: u64,
    pub locations: Vec<ActivityToolLocation>,
    pub content: Vec<ActivityToolContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<ActivityToolInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<ActivityToolOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminal_outputs: Vec<TerminalOutputSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputSnapshot {
    pub terminal_id: String,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActivityToolLocation {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ActivityToolContent {
    Text {
        text: String,
    },
    Diff {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        old_text: Option<String>,
        new_text: String,
    },
    Terminal {
        terminal_id: String,
    },
    Image {
        media_type: String,
        data_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
    },
    Audio {
        media_type: String,
        data_url: String,
    },
    Resource {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size_bytes: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Unsupported {
        content_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActivityToolInput {
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queries: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub fields: Vec<ActivityToolField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActivityToolOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatted_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregated_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    pub fields: Vec<ActivityToolField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActivityToolField {
    pub name: String,
    pub value: ActivityToolValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ActivityToolValue {
    Null,
    Boolean { value: bool },
    Number { value: String },
    String { value: String },
    Array { items: Vec<ActivityToolValue> },
    Object { fields: Vec<ActivityToolField> },
    Redacted,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskListParams {
    #[serde(default)]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<TaskListCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskListResult {
    pub tasks: Vec<TaskSummary>,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<TaskListCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskReleaseParams {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskReleaseResult {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetArchivedParams {
    pub task_id: TaskId,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetArchivedResult {
    pub task_id: TaskId,
    pub archived: bool,
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
