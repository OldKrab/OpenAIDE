use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::client::SettingsSection;
use crate::events::TaskChatChange;
use crate::ids::{EventCursor, ProjectId, SubagentId, TaskId, WorktreeRepositoryId};
use crate::snapshot::{
    AgentCollectionSnapshot, ProjectCollectionSnapshot, SettingsSnapshot, SubagentCatalogSnapshot,
    SubagentHistorySnapshot, TaskNavigationSnapshot, TaskSnapshot,
};
use crate::task::TaskNavigationSection;
use crate::task::ToolDetailSnapshot;
use crate::worktree::WorktreeRepositorySnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StateSubscribeParams {
    pub scope: SubscriptionScope,
    /// The content version of a copy this client already holds (ADR-0086): for a Task the number
    /// of the last Chat frame it has applied, for a Subagent History its revision, for a Tool
    /// detail its artifact sequence. The reply says whether the copy is current, which changes
    /// it misses, or that it must take the whole content. Versions are durable, so they stay
    /// valid across App Server restarts. Other scopes ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub known_version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StateSubscribeResult {
    pub cursor: EventCursor,
    pub scope: SubscriptionScope,
    pub snapshot: SubscriptionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StateUnsubscribeParams {
    pub scope: SubscriptionScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StateUnsubscribeResult {
    pub scope: SubscriptionScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SubscriptionScope {
    Projects,
    Agents,
    Settings {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<SettingsSection>,
    },
    TaskNavigation {
        #[serde(default)]
        section: TaskNavigationSection,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_ids: Option<Vec<ProjectId>>,
    },
    Task {
        task_id: TaskId,
    },
    SubagentCatalog {
        task_id: TaskId,
    },
    SubagentHistory {
        task_id: TaskId,
        subagent_id: SubagentId,
    },
    ToolDetail {
        task_id: TaskId,
        artifact_id: String,
    },
    WorktreeRepository {
        repository_id: WorktreeRepositoryId,
    },
    /// The Remote Devices this App Server trusts. Every client may subscribe.
    Devices,
    /// Every open Terminal on this App Server. Every client may subscribe.
    Terminals,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
// Subscription snapshots are protocol values whose direct shape is shared with
// generated clients, so variant size is secondary to a consistent boundary.
#[allow(clippy::large_enum_variant)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SubscriptionSnapshot {
    Projects {
        projects: ProjectCollectionSnapshot,
    },
    Agents {
        agents: AgentCollectionSnapshot,
    },
    Settings {
        settings: SettingsSnapshot,
    },
    TaskNavigation {
        navigation: TaskNavigationSnapshot,
    },
    /// Every current Task field, with the Chat as `chatSync` says.
    Task {
        /// Complete and current. `task.chat` is the tail page only when `chatSync` is `full`;
        /// otherwise it is an empty placeholder and the client keeps its own Chat.
        task: TaskSnapshot,
        /// The Chat version after the Chat this reply describes. Later `taskChanged`
        /// notifications carry the version their Chat changes lead to.
        chat_version: u64,
        chat_sync: TaskChatSync,
    },
    SubagentCatalog {
        catalog: SubagentCatalogSnapshot,
    },
    /// Every current field, with the Chat as `chatSync` says.
    SubagentHistory {
        /// Complete and current. `history.chat` is the tail page only when `chatSync` is
        /// `full`; otherwise it is an empty placeholder and the client keeps its own Chat.
        /// `history.revision` is the version later `subagentHistoryChanged` notifications follow.
        history: SubagentHistorySnapshot,
        chat_sync: SubagentChatSync,
    },
    /// Every current field, with the terminal output as `terminalSync` says.
    ToolDetail {
        task_id: TaskId,
        artifact_id: String,
        /// Complete and current. `details.terminalOutputs` holds the whole output only when
        /// `terminalSync` is `full`; otherwise it is empty and the client keeps its own.
        /// `details.revision` is the artifact sequence later notifications follow.
        details: ToolDetailSnapshot,
        terminal_sync: ToolTerminalSync,
    },
    WorktreeRepository {
        repository: WorktreeRepositorySnapshot,
    },
    Devices {
        devices: crate::devices::DeviceCollectionSnapshot,
    },
    Terminals {
        terminals: crate::terminals::TerminalCollectionSnapshot,
    },
}

/// How a Task subscription reply brings the client's Chat to `chatVersion`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskChatSync {
    /// The client's known version is the latest; its Chat is unchanged.
    Current,
    /// The operations of the Chat frames after the known version, projected and in order.
    Deltas { changes: Vec<TaskChatChange> },
    /// `task.chat` is the tail page and replaces the client's Chat: no version was offered, it
    /// predates the last compaction or replacement, or it is ahead of the Task.
    Full,
}

/// How a Subagent History subscription reply brings the client's Chat to `history.revision`.
/// Deltas are not offered: a history is a bounded tail and its change journal restarts at every
/// compaction, so serving them would need a change log App Server does not keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SubagentChatSync {
    /// The client's known version is the latest; its Chat is unchanged.
    Current,
    /// `history.chat` is the tail page and replaces the client's Chat: no version was offered,
    /// or it differs from the history's.
    Full,
}

/// How a Tool detail subscription reply brings the client's terminal output to
/// `details.revision`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolTerminalSync {
    /// The client's known version is the latest; its output is unchanged.
    Current,
    /// The terminal output appended after the known version, in order.
    Deltas { appends: Vec<TerminalOutputAppend> },
    /// `details.terminalOutputs` replaces the client's output: no version was offered, or it
    /// is ahead of the artifact.
    Full,
}

/// Terminal output added to one terminal of a Tool detail.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputAppend {
    pub terminal_id: String,
    pub data: String,
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
