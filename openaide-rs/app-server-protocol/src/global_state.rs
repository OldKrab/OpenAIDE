//! The global state (ADR-0086): small app-wide data every client holds a full copy of.
//!
//! `client/initialize` returns it complete and App Server pushes each change as one `sync`
//! notification. There is no change log and no sync position: a client that loses its session
//! initializes again and receives the whole state.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::devices::DeviceCollectionSnapshot;
use crate::ids::WorktreeRepositoryId;
use crate::snapshot::{
    AgentCollectionSnapshot, NewTaskDefaultsSnapshot, ProjectCollectionSnapshot, SettingsSnapshot,
};
use crate::terminals::TerminalCollectionSnapshot;
use crate::worktree::WorktreeRepositorySnapshot;

/// Wire method of the server-to-client notification that carries [`SyncNotification`].
pub const SYNC_NOTIFICATION: &str = "sync";

/// Everything a client needs from App Server that is not Task Navigation or an open entity.
/// Settings carry preferences and runtime settings; Skills and Agent details stay requests made
/// by the Settings page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GlobalState {
    pub projects: ProjectCollectionSnapshot,
    /// Every Worktree Repository a Project uses, keyed by `repositoryId`. A repository can serve
    /// several Projects and carries the live linked and running Task counts.
    #[serde(default)]
    pub worktrees: Vec<WorktreeRepositorySnapshot>,
    /// Agents, each with its recovery state (status, setup reason, Sign-in Flow, account limits).
    pub agents: AgentCollectionSnapshot,
    pub settings: SettingsSnapshot,
    pub new_task_defaults: NewTaskDefaultsSnapshot,
    pub devices: DeviceCollectionSnapshot,
    pub terminals: TerminalCollectionSnapshot,
}

/// The areas of the global state that changed, applied by a client in one step. An absent area is
/// unchanged; a present one replaces the client's copy, except worktrees, which are keyed by
/// repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStateChanges {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<ProjectCollectionSnapshot>,
    /// Repositories that are new or changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worktrees: Vec<WorktreeRepositorySnapshot>,
    /// Repositories no Project uses any more.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_worktree_repositories: Vec<WorktreeRepositoryId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<AgentCollectionSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<SettingsSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_task_defaults: Option<NewTaskDefaultsSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devices: Option<DeviceCollectionSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminals: Option<TerminalCollectionSnapshot>,
}

impl GlobalStateChanges {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// The `sync` notification: one change to the global state, spanning any number of areas.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SyncNotification {
    pub changes: GlobalStateChanges,
}

#[cfg(test)]
#[path = "global_state_tests.rs"]
mod tests;
