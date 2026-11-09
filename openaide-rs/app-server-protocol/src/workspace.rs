use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListRootsParams {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListRootsResult {
    pub roots: Vec<WorkspaceBrowserRoot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBrowserRoot {
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListDirectoryParams {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListDirectoryResult {
    pub directory: WorkspaceBrowserDirectory,
    pub entries: Vec<WorkspaceBrowserEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBrowserDirectory {
    pub label: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBrowserEntry {
    pub label: String,
    pub path: String,
}

/// Creates one new, empty folder inside `parent_path` for the Add Project explorer.
///
/// `name` is a single path segment. The App Server never creates intermediate
/// folders and never reuses an existing one, so a typo cannot silently select a
/// different folder. The result lists the created folder, so the explorer opens it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCreateDirectoryParams {
    pub parent_path: String,
    pub name: String,
}
