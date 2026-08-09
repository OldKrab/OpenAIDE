use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::attachment::PreSendAttachment;
use crate::ids::{
    AgentConfigOptionId, AgentId, AttachmentHandleId, ClientMutationId, MessageId, ProjectId,
    QueuedMessageId, TaskId, TaskListCursor, TurnId, WorktreeId,
};
use crate::snapshot::AgentConfigOptionCurrentValue;
use crate::snapshot::{
    ChatItem, TaskAgentConfigSnapshot, TaskLifecycle, TaskSnapshot, TaskSummary,
};

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
    pub agent_id: AgentId,
    pub native_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAdoptNativeSessionResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(rename_all = "camelCase")]
pub enum NativeSessionForkSource {
    Task {
        task_id: TaskId,
    },
    NativeSession {
        agent_id: AgentId,
        native_session_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionForkParams {
    pub source: NativeSessionForkSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionForkResult {
    pub reference: crate::snapshot::NativeSessionReference,
    pub project_id: ProjectId,
    pub close_warning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSendParams {
    pub task_id: TaskId,
    pub message: ComposerMessage,
    /// Removes this exact durable queue item in the same commit that accepts Send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_selection: Option<TaskQueueSendSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueSendSelection {
    pub queued_message_id: QueuedMessageId,
    pub queue_revision: u64,
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
pub struct TaskQueueAppendParams {
    pub task_id: TaskId,
    pub message: ComposerMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueAppendResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueRemoveParams {
    pub task_id: TaskId,
    pub queued_message_id: QueuedMessageId,
    pub queue_revision: u64,
    pub client_mutation_id: ClientMutationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueRemoveResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueTakeParams {
    pub task_id: TaskId,
    pub queued_message_id: QueuedMessageId,
    pub queue_revision: u64,
    pub client_mutation_id: ClientMutationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueTakeResult {
    pub task: TaskSnapshot,
    pub message: TakenQueuedMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TakenQueuedMessage {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<PreSendAttachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ComposerImage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueMoveParams {
    pub task_id: TaskId,
    pub queued_message_id: QueuedMessageId,
    pub target_index: u64,
    pub queue_revision: u64,
    pub client_mutation_id: ClientMutationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueMoveResult {
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComposerHistoryScope {
    Task { task_id: TaskId },
    Project { project_id: ProjectId },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComposerHistoryParams {
    pub scope: ComposerHistoryScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComposerHistoryEntry {
    pub entry_id: String,
    pub text: String,
    pub accepted_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComposerHistoryResult {
    pub entries: Vec<ComposerHistoryEntry>,
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
    /// Complete Agent-owned Configuration Option state confirmed by the mutation.
    pub agent_config: TaskAgentConfigSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetTitleParams {
    pub task_id: TaskId,
    pub title: TaskTitleSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskTitleSelection {
    User { value: String },
    Automatic,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetTitleResult {
    pub task: TaskSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetPinnedParams {
    pub task_id: TaskId,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskSetPinnedResult {
    pub task: TaskSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskClosePlanParams {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskClosePlanResult {
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
pub struct TaskToolImagePreviewParams {
    pub task_id: TaskId,
    pub artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskToolImagePreviewResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<ToolImagePreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolImagePreview {
    pub label: String,
    pub media_type: String,
    pub data_url: String,
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
    pub lifecycle: TaskListLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<TaskListCursor>,
}

/// Selects one authoritative visible Task collection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskListLifecycle {
    #[default]
    Open,
    Archived,
}

/// Selects the primary Task navigation or its secondary read-only Archive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskNavigationSection {
    #[default]
    Tasks,
    Archive,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskListResult {
    pub tasks: Vec<TaskSummary>,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<TaskListCursor>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskNavigationRefreshParams {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskNavigationRefreshResult {
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskNavigationLoadMoreParams {
    pub project_id: ProjectId,
    pub target_row_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskNavigationLoadMoreResult {
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionArchiveParams {
    pub agent_id: AgentId,
    pub native_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionArchiveResult {
    pub reference: crate::snapshot::NativeSessionReference,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionRestoreParams {
    pub agent_id: AgentId,
    pub native_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionRestoreResult {
    pub reference: crate::snapshot::NativeSessionReference,
    pub archived: bool,
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
pub struct TaskArchiveParams {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskArchiveResult {
    pub change: TaskLifecycleChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskRestoreParams {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskRestoreResult {
    pub change: TaskLifecycleChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskLifecycleChanged {
    pub previous_lifecycle: TaskLifecycle,
    pub task: TaskSummary,
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
