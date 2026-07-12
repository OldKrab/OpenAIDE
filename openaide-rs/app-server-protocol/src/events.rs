use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ClientInstanceId, EventCursor, MessageId, StateRootId, TaskId};
use crate::snapshot::{
    AgentCollectionSnapshot, ChatItem, ClientSnapshot, PendingRequestSnapshot,
    ProjectCollectionSnapshot, TaskNavigationSnapshot, TaskSnapshot, TaskSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppServerEvent {
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
    TaskUpdated {
        task: TaskSummary,
    },
    TaskSnapshotUpdated {
        task: TaskSnapshot,
    },
    TaskHistorySyncUpdated {
        task_id: TaskId,
        history_sync: crate::snapshot::TaskHistorySyncSnapshot,
    },
    TaskNavigationUpdated {
        navigation: TaskNavigationSnapshot,
    },
    ProjectCollectionUpdated {
        projects: ProjectCollectionSnapshot,
    },
    // Hot Chat deltas carry the resulting durable Task revision so replicas advance with the stream.
    ChatItemAppended {
        task_id: TaskId,
        revision: u64,
        item: ChatItem,
    },
    ChatItemChunk {
        task_id: TaskId,
        revision: u64,
        message_id: MessageId,
        chunk: TextChunk,
    },
    RequestUpdated {
        request: PendingRequestSnapshot,
    },
    AgentCollectionUpdated {
        agents: AgentCollectionSnapshot,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TextChunk {
    pub sequence: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub final_chunk: bool,
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
