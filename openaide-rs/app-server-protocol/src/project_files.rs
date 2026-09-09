//! Read-only discovery in an authorized Task Workspace. Cursors are best-effort offsets
//! into the current filesystem, not persistent snapshots; refresh after filesystem changes.
//! Pages contain up to 200 entries. Discovery scans at most 20,000 entries or 2 seconds;
//! content search reads at most 1 MiB per UTF-8 text file and 64 MiB total. Symlinks are
//! not followed by search. The tree includes ignored files, excluding .git and escaping
//! symlinks; search respects Git and .ignore rules by default, including hidden files.
//! Git operations require the workspace to be a repository root, cap each subprocess
//! at 5 seconds / 2 MiB output, and disable external diff drivers and text conversion.
//! Superseded requests may finish their bounded work; clients discard obsolete results.
use crate::ids::TaskId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFilesParams {
    pub task_id: TaskId,
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
