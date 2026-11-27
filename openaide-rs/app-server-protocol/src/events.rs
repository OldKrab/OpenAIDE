use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ClientInstanceId, EventCursor, MessageId, StateRootId, SubagentId, TaskId};
use crate::snapshot::{
    AgentPlanSnapshot, BackgroundCommandSnapshot, ChatItem, ChatSnapshot, PendingRequestSnapshot,
    SubagentCatalogSnapshot, TaskAgentCommandsSnapshot, TaskAgentConfigSnapshot, TaskContextUsage,
    TaskHistorySyncSnapshot, TaskInputCapabilities, TaskLifecycle, TaskMessageQueueSnapshot,
    TaskPermissionPolicy, TaskPreparationSnapshot, TaskSendCapabilitySnapshot, TaskSummary,
};
use crate::state::SubscriptionScope;
use crate::task::ToolDetailSnapshot;

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
    TaskChanged {
        task_id: TaskId,
        revision: u64,
        changes: TaskChanges,
    },
    TaskHistorySyncUpdated {
        task_id: TaskId,
        history_sync: TaskHistorySyncSnapshot,
    },
    /// One Navigation view changed (design 2.2). Frames also carry the subscribe baseline.
    NavigationFrame {
        frame: crate::navigation::NavigationFrame,
    },
    TaskRequestsUpdated {
        task_id: TaskId,
        requests: Vec<PendingRequestSnapshot>,
    },
    SubagentCatalogUpdated {
        catalog: SubagentCatalogSnapshot,
    },
    /// Advances one Subagent History by exactly one revision (ADR-0046). A client
    /// whose copy is not at `revision - 1` re-reads the history instead.
    SubagentHistoryChanged {
        task_id: TaskId,
        subagent_id: SubagentId,
        revision: u64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        chat: Vec<TaskChatChange>,
        /// Outer option controls delta presence; inner option clears the Agent Plan.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_plan: Option<Option<AgentPlanSnapshot>>,
    },
    /// The Task is deleted, so its Subagent catalog, Subagent Histories and Tool details are
    /// gone. The final notification of each of their subscriptions.
    TaskContentRemoved {
        task_id: TaskId,
    },
    ToolDetailChanged {
        task_id: TaskId,
        artifact_id: String,
        revision: u64,
        deltas: Vec<ToolDetailDelta>,
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
    /// Present with every Task summary change; replaces the whole list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_commands: Option<Vec<BackgroundCommandSnapshot>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<TaskLifecycle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation: Option<TaskPreparationSnapshot>,
    /// Task-owned permission handling. Carried as a delta so an open replica
    /// learns about `task/setPermissionPolicy` without a fresh baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<TaskPermissionPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_config: Option<TaskAgentConfigSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_commands: Option<TaskAgentCommandsSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_capability: Option<TaskSendCapabilitySnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_capabilities: Option<TaskInputCapabilities>,
    /// Outer option controls delta presence; inner option clears stale session usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage: Option<Option<TaskContextUsage>>,
    /// Outer option controls delta presence; inner option clears the current Agent Plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_plan: Option<Option<AgentPlanSnapshot>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_queue: Option<TaskMessageQueueSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat: Vec<TaskChatChange>,
    /// The Chat version after `chat`: the number of the Chat frame the transaction committed.
    /// Present exactly when `chat` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_version: Option<u64>,
    /// The Task is deleted. This is the subscription's final notification.
    #[serde(default, skip_serializing_if = "is_false")]
    pub removed: bool,
}

impl TaskChanges {
    pub fn is_empty(&self) -> bool {
        self.task.is_none()
            && self.active_turn_started_at.is_none()
            && self.lifecycle.is_none()
            && self.preparation.is_none()
            && self.permission_policy.is_none()
            && self.agent_config.is_none()
            && self.agent_commands.is_none()
            && self.send_capability.is_none()
            && self.input_capabilities.is_none()
            && self.context_usage.is_none()
            && self.current_plan.is_none()
            && self.message_queue.is_none()
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

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
