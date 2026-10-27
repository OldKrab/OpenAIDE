//! Read-only discovery in one authorized workspace: a Task Workspace, a Project root, or a
//! worktree of the same repository selected by identifier. The browser never names a root
//! by path. Cursors are best-effort offsets
//! into the current filesystem, not persistent snapshots; refresh after filesystem changes.
//! Pages contain up to 200 entries. Discovery scans at most 20,000 entries or 2 seconds;
//! content search reads at most 1 MiB per UTF-8 text file and 64 MiB total. Symlinks are
//! not followed by search. The tree includes ignored files, excluding .git and escaping
//! symlinks; search respects Git and .ignore rules by default, including hidden files.
//! A file-name search matches the query, or its characters in order, against each path and
//! returns matches best first (file name equal, prefix, contained, then path), shorter paths
//! first within a tier. Content search returns matching lines in walk order.
//! Git operations require the workspace to be a repository root, cap each subprocess
//! at 5 seconds / 2 MiB output, and disable external diff drivers and text conversion.
//! Changes compare the working tree with HEAD, or with the merge base of HEAD and the
//! default branch (the remote-tracking one when it exists; nothing is fetched). Line counts
//! are omitted for binary, conflicted, and oversized files.
//! Superseded requests may finish their bounded work; clients discard obsolete results.
//! `fileViewer/existingFiles` answers which of up to 100 workspace-relative paths name a
//! regular file inside the workspace, in request order, so Chat links only real files.
use crate::ids::{ProjectId, TaskId, WorktreeId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFilesParams {
    /// Exactly one of `task_id` and `project_id` names the scope: a Task authorizes its Task
    /// Workspace, a Project its root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_id: Option<ProjectId>,
    /// Selects another worktree of the scope's repository instead of its default root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub worktree_id: Option<WorktreeId>,
    /// What changes and diffs compare against. Absent means uncommitted work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<ProjectChangeBase>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub content: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    /// Search respects ignore files unless explicitly requested. Tree browsing includes ignored files.
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default)]
    pub cursor: u32,
    /// `fileViewer/existingFiles` only: workspace-relative paths to check, at most 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub paths: Option<Vec<String>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ProjectChangeBase {
    Uncommitted,
    DefaultBranch,
}
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileEntry {
    pub path: String,
    pub name: String,
    pub directory: bool,
    pub status: Option<String>,
    pub old_path: Option<String>,
    pub line: Option<u32>,
    pub text: Option<String>,
    /// Lines added and removed against the requested base; changes only.
    pub added: Option<u32>,
    pub removed: Option<u32>,
}
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFilesResult {
    pub entries: Vec<ProjectFileEntry>,
    pub next_cursor: Option<u32>,
    /// True when scan, time, output or file-size limits prevent a complete result.
    pub truncated: bool,
    /// Classified code only: unavailable, permissionDenied, outsideWorkspace, notRepository, timeout, tooLarge, changed.
    pub error: Option<String>,
    pub branch: Option<String>,
    pub base: Option<String>,
    /// Short name of the default branch when comparing against it would differ from
    /// uncommitted work; absent on that branch itself or when none can be found.
    pub default_branch: Option<String>,
    /// Number of changed files before paging; changes only.
    pub total: Option<u32>,
    pub diff: Option<ProjectFileDiff>,
}
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub binary: bool,
    pub conflicted: bool,
    pub hunks: Vec<ProjectDiffHunk>,
}
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiffHunk {
    pub heading: String,
    pub lines: Vec<ProjectDiffLine>,
}
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiffLine {
    pub kind: String,
    pub text: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}
