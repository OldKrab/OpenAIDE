use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{
    ClientInstanceId, EventCursor, MessageId, StateRootId, TaskId, WorktreeRepositoryId,
};
use crate::snapshot::{
    AgentCollectionSnapshot, ChatItem, ChatSnapshot, ClientSnapshot, PendingRequestSnapshot,
    ProjectCollectionSnapshot, TaskAgentCommandsSnapshot, TaskAgentConfigSnapshot,
    TaskHistorySyncSnapshot, TaskInputCapabilities, TaskLifecycle, TaskNavigationSnapshot,
    TaskPreparationSnapshot, TaskSendCapabilitySnapshot, TaskSummary,
};
use crate::state::SubscriptionScope;
use crate::task::{TaskLifecycleChanged, ToolDetailSnapshot};
use crate::worktree::WorktreeRepositorySnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppServerEvent {
    /// Identifies the independently ordered subscription stream carrying this event.
    pub subscription: SubscriptionScope,
    pub previous_cursor: EventCursor,
    pub cursor: EventCursor,
    pub scope: EventScope,
    pub payload: AppServerEventPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EventScope {
    StateRoot {
        state_root_id: StateRootId,
    },
    Client {
        state_root_id: StateRootId,
        client_instance_id: ClientInstanceId,
    },
    Task {
        state_root_id: StateRootId,
        task_id: TaskId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
// Protocol payloads stay direct value types so Rust and generated TypeScript
// contracts retain the same visible ownership model across event variants.
#[allow(clippy::large_enum_variant)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AppServerEventPayload {
    SnapshotReplaced {
        snapshot: ClientSnapshot,
    },
    TaskChanged {
        task_id: TaskId,
        revision: u64,
        changes: TaskChanges,
    },
    TaskHistorySyncUpdated {
        task_id: TaskId,
        history_sync: TaskHistorySyncSnapshot,
    },
    TaskNavigationChanged {
        change: TaskNavigationChange,
    },
    TaskLifecycleChanged {
        change: TaskLifecycleChanged,
    },
    /// Replaces the combined durable Task and unadopted Native Session projection.
    TaskNavigationReplaced {
        navigation: TaskNavigationSnapshot,
    },
    ProjectCollectionUpdated {
        projects: ProjectCollectionSnapshot,
    },
    TaskRequestsUpdated {
        task_id: TaskId,
        requests: Vec<PendingRequestSnapshot>,
    },
    ToolDetailUpdated {
        task_id: TaskId,
        artifact_id: String,
        details: ToolDetailSnapshot,
    },
    ToolDetailChanged {
        task_id: TaskId,
        artifact_id: String,
        revision: u64,
        deltas: Vec<ToolDetailDelta>,
    },
    RequestUpdated {
        request: PendingRequestSnapshot,
    },
    AgentCollectionUpdated {
        agents: AgentCollectionSnapshot,
    },
    WorktreeRepositoryUpdated {
        repository_id: WorktreeRepositoryId,
        repository: WorktreeRepositorySnapshot,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolDetailDelta {
    ReplaceDetails { details: Box<ToolDetailSnapshot> },
    AppendTerminal { terminal_id: String, data: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskChanges {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskSummary>,
    /// Present when the active-turn clock changes; inner `None` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_started_at: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<TaskLifecycle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation: Option<TaskPreparationSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_config: Option<TaskAgentConfigSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_commands: Option<TaskAgentCommandsSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_capability: Option<TaskSendCapabilitySnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_capabilities: Option<TaskInputCapabilities>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat: Vec<TaskChatChange>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub removed: bool,
}

impl TaskChanges {
    pub fn is_empty(&self) -> bool {
        self.task.is_none()
            && self.active_turn_started_at.is_none()
            && self.lifecycle.is_none()
            && self.preparation.is_none()
            && self.agent_config.is_none()
            && self.agent_commands.is_none()
            && self.send_capability.is_none()
            && self.input_capabilities.is_none()
            && self.chat.is_empty()
            && !self.removed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskChatChange {
    Append { item: ChatItem },
    Upsert { item: ChatItem },
    AppendText { message_id: MessageId, text: String },
    Replace { chat: ChatSnapshot },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskNavigationChange {
    Upsert { task: Box<TaskSummary> },
    Remove { task_id: TaskId },
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
