//! Task Navigation as a server-computed view subscription (ADR-0086, design 2.2).
//!
//! A client subscribes to one Navigation view per section and describes the window it wants.
//! App Server owns membership, ordering and the window predicate; it pushes `NavigationFrame`s
//! that carry the full order of each changed group and bodies only for new or changed rows.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AgentId, ProjectId, TaskId};
use crate::snapshot::TaskNavigationEntry;
use crate::task::TaskNavigationSection;

/// Names the one row the client has open, so the server keeps it in the window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NavigationRowRef {
    Task {
        task_id: TaskId,
    },
    NativeSession {
        agent_id: AgentId,
        session_id: String,
    },
}

/// A per-Project history limit that replaces the view-wide one for that Project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationHistoryLimit {
    pub project_id: ProjectId,
    pub history_limit: u32,
}

/// The parameters of a Navigation view, shared by `navigation/subscribe` and `navigation/update`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationViewParams {
    #[serde(default)]
    pub section: TaskNavigationSection,
    /// Limits the view to these Projects. Absent means every Project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_ids: Option<Vec<ProjectId>>,
    /// How many non-live (history) rows each Project shows.
    pub history_limit: u32,
    /// Per-Project overrides of `historyLimit`.
    #[serde(default)]
    pub history_limits: Vec<NavigationHistoryLimit>,
    /// The open row, which stays in the window whatever the limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub open_row: Option<NavigationRowRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationSubscribeResult {
    /// The baseline: every group of the view as a frame.
    pub frame: NavigationFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationUpdateResult {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationUnsubscribeParams {
    #[serde(default)]
    pub section: TaskNavigationSection,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationUnsubscribeResult {}

/// Whether App Server has finished discovering a Project's Native Sessions for this view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum NavigationDiscovery {
    #[default]
    Settled,
    Loading,
    Failed,
}

/// One row body. `live` is true when the row is outside the history limit by right.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationRow {
    pub live: bool,
    pub entry: TaskNavigationEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationGroupFrame {
    pub project_id: ProjectId,
    pub project_label: String,
    /// The complete window order, as row ids (`task:<id>` or `session:<agent>:<id>`).
    pub order: Vec<String>,
    /// Bodies for rows that are new or changed since the last frame of this view.
    #[serde(default)]
    pub rows: std::collections::BTreeMap<String, NavigationRow>,
    /// Rows the Project has in this section that pass the membership filters.
    pub total: u64,
    /// Rows that pass the filters but sit outside the window.
    pub hidden: u64,
    /// True when more history may exist in an Agent that has not been fully read.
    #[serde(default)]
    pub undiscovered: bool,
    #[serde(default)]
    pub discovery: NavigationDiscovery,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NavigationFrame {
    pub section: TaskNavigationSection,
    /// Orders frames across views and subscriptions of one App Server run: it only grows, and a
    /// baseline carries the value current when it was computed. A client drops a frame whose
    /// revision is not above the one it holds, which discards frames sent before its baseline.
    #[serde(default)]
    pub revision: u64,
    /// The Project order, present when it changed (always in the baseline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub group_order: Option<Vec<ProjectId>>,
    /// Groups removed from the view.
    #[serde(default)]
    pub removed_groups: Vec<ProjectId>,
    /// Only the groups that changed.
    #[serde(default)]
    pub groups: Vec<NavigationGroupFrame>,
}
