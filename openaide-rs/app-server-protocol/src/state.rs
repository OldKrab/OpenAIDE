use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::client::SettingsSection;
use crate::ids::{EventCursor, ProjectId, TaskId, WorktreeRepositoryId};
use crate::snapshot::{
    AgentCollectionSnapshot, ProjectCollectionSnapshot, SettingsSnapshot, TaskNavigationSnapshot,
    TaskSnapshot,
};
use crate::task::ToolDetailSnapshot;
use crate::worktree::WorktreeRepositorySnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StateSubscribeParams {
    pub scope: SubscriptionScope,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    Task {
        task_id: TaskId,
    },
    ToolDetail {
        task_id: TaskId,
        artifact_id: String,
    },
    WorktreeRepository {
        repository_id: WorktreeRepositoryId,
    },
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
    ToolDetail {
        task_id: TaskId,
        artifact_id: String,
        details: ToolDetailSnapshot,
    },
    WorktreeRepository {
        repository: WorktreeRepositorySnapshot,
    },
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
