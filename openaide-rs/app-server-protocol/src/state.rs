use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::client::SettingsSection;
use crate::ids::{EventCursor, ProjectId, TaskId};
use crate::snapshot::{
    AgentCollectionSnapshot, ProjectCollectionSnapshot, SettingsSnapshot, TaskNavigationSnapshot,
    TaskSnapshot,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SubscriptionSnapshot {
    Projects { projects: ProjectCollectionSnapshot },
    Agents { agents: AgentCollectionSnapshot },
    Settings { settings: SettingsSnapshot },
    TaskNavigation { navigation: TaskNavigationSnapshot },
    Task { task: TaskSnapshot },
}

#[cfg(test)]
mod tests;
