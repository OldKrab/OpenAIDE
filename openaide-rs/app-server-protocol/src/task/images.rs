//! Loading the images a Task's Chat and Tool details name (ADR-0075, ADR-0084).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::TaskId;

/// Loads one stored image of a Task by the id a Chat reference names (ADR-0075).
/// Its Task Chat or a Subagent history of that Task may reference it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskImageParams {
    pub task_id: TaskId,
    pub image_id: String,
    /// Which copy to return. Absent means the original, as before previews existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub variant: Option<TaskImageVariant>,
}

/// The copy of a stored image that `task/image` returns (ADR-0084).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskImageVariant {
    /// The stored bytes, for inspecting the image at full size.
    Original,
    /// A copy no larger than a Chat row shows. App Server returns the original
    /// instead when it is already that small or a smaller copy would not save bytes.
    Preview,
}

/// The image's bytes as base64. The reference that named the image carries its
/// media type, and an id always names the same bytes, so a client may cache it
/// per variant.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskImageResult {
    pub data: String,
    /// Set only when `data` is a preview App Server encoded again, whose media
    /// type can differ from the reference's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskToolImagePreviewParams {
    pub task_id: TaskId,
    pub artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskToolImagePreviewResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<ToolImagePreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolImagePreview {
    pub label: String,
    pub media_type: String,
    pub data_url: String,
}
