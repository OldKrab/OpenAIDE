use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{FileViewerHandleId, TaskId};
use crate::task::ToolImagePreview;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileViewerOpenParams {
    pub task_id: TaskId,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileViewerOpenFromHandleParams {
    pub handle: FileViewerHandleId,
    pub href: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileViewerRefreshParams {
    pub handle: FileViewerHandleId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileViewerReleaseParams {
    pub handle: FileViewerHandleId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileViewerReleaseResult {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileViewerSnapshot {
    pub handle: FileViewerHandleId,
    pub display_path: String,
    pub basename: String,
    pub kind: FileViewerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<ToolImagePreview>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<FileViewerError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FileViewerKind {
    Markdown,
    Source,
    Image,
    Binary,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FileViewerError {
    NotFound,
    PermissionDenied,
    NotAFile,
    Unsupported,
    Unreadable,
}
