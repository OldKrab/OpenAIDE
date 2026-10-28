use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::client::SettingsSection;
use crate::events::TaskChanges;
use crate::ids::{EventCursor, ProjectId, SubagentId, TaskId, WorktreeRepositoryId};
use crate::snapshot::{
    AgentCollectionSnapshot, PendingRequestSnapshot, ProjectCollectionSnapshot, SettingsSnapshot,
    SubagentCatalogSnapshot, SubagentHistorySnapshot, SubagentOverviewSnapshot,
    TaskHistorySyncSnapshot, TaskNavigationSnapshot, TaskSnapshot,
};
use crate::task::TaskNavigationSection;
use crate::task::ToolDetailSnapshot;
use crate::worktree::WorktreeRepositorySnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StateSubscribeParams {
    pub scope: SubscriptionScope,
    /// Task revision of a copy this client already holds from the current App Server
    /// process. Only a Task scope honors it; the reply may then be a `taskResume`
    /// instead of a complete baseline (ADR-0073).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub known_revision: Option<u64>,
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
    Task {
        task: TaskSnapshot,
    },
    /// Brings the client's copy at `knownRevision` up to date without a baseline.
    TaskResume {
        resume: TaskResumeSnapshot,
    },
    SubagentCatalog {
        catalog: SubagentCatalogSnapshot,
    },
    SubagentHistory {
        history: SubagentHistorySnapshot,
    },
    ToolDetail {
        task_id: TaskId,
        artifact_id: String,
        details: ToolDetailSnapshot,
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

/// The client's Task copy at `known_revision`, plus `changes` applied in order, is the
/// Task at the reply's position in its ordered stream. Later revisions arrive as live
/// `taskChanged` events. The remaining fields change without a Task revision, so they
/// always carry their current values and replace the copy's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskResumeSnapshot {
    pub task_id: TaskId,
    pub known_revision: u64,
    /// Published `taskChanged` changes after `known_revision`; empty when the copy is current.
    pub changes: Vec<TaskRevisionChanges>,
    pub history_sync: TaskHistorySyncSnapshot,
    pub pending_requests: Vec<PendingRequestSnapshot>,
    pub subagents: SubagentOverviewSnapshot,
    pub workspace_available: bool,
}

/// One published `taskChanged` event, kept so a returning client can replay it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskRevisionChanges {
    pub revision: u64,
    pub changes: TaskChanges,
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
