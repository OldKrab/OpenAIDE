//! Terminals: the user's own shells on the App Server's computer (ADR-0071).
//!
//! The App Server Protocol carries lifecycle and the published Terminal list. Keystrokes and
//! output travel on a dedicated per-Terminal WebSocket that a client opens with the single-use
//! attach token these methods return, so terminal bytes never queue behind other traffic.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ProjectId, TaskId};

/// Where a new Terminal starts. App Server resolves the folder; clients never send a path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TerminalTarget {
    /// The Project root.
    Project { project_id: ProjectId },
    /// The Task Workspace, which may be a worktree of the Task's Project.
    Task { task_id: TaskId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TerminalStatus {
    /// The shell is waiting at its prompt.
    Idle,
    /// A foreground command other than the shell is running.
    Busy,
    /// The shell ended; the Terminal stays listed until the user closes it.
    Exited,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSummary {
    pub terminal_id: String,
    /// The Project the Terminal was opened from, for grouping and counts.
    pub project_id: ProjectId,
    /// The folder the shell started in, for display.
    pub folder: String,
    /// The foreground command's name while one runs, when the platform reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub status: TerminalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Milliseconds since the Unix epoch; orders tabs identically on every client.
    pub opened_at: u64,
}

/// Every open Terminal on this App Server, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCollectionSnapshot {
    pub terminals: Vec<TerminalSummary>,
}

/// Authorizes one WebSocket attach to one Terminal. The token is single-use and short-lived;
/// a reconnecting client asks for a new one with `terminals/attach`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttach {
    pub terminal_id: String,
    pub token: String,
    /// Path on the App Server's local HTTP origin that accepts the WebSocket upgrade.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalsOpenParams {
    pub target: TerminalTarget,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalsOpenResult {
    pub terminal: TerminalSummary,
    pub attach: TerminalAttach,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalsAttachParams {
    pub terminal_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalsAttachResult {
    pub attach: TerminalAttach,
}

/// Closing a busy Terminal ends its command, so it fails with `conflict` unless `force` is set
/// after the user confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalsCloseParams {
    pub terminal_id: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalsCloseResult {}
