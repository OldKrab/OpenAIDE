use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{
    AttachmentCandidateId, AttachmentHandleId, FileBrowserEntryId, FileBrowserRootId, TaskId,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentListRootsParams {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentListRootsResult {
    pub roots: Vec<FileBrowserRoot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileBrowserRoot {
    pub root_id: FileBrowserRootId,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentListDirectoryParams {
    pub task_id: TaskId,
    pub root_id: FileBrowserRootId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory_id: Option<FileBrowserEntryId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentListDirectoryResult {
    pub directory: FileBrowserDirectory,
    pub entries: Vec<FileBrowserEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileBrowserDirectory {
    pub label: String,
    pub root_id: FileBrowserRootId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory_id: Option<FileBrowserEntryId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileBrowserEntry {
    pub entry_id: FileBrowserEntryId,
    pub label: String,
    pub kind: FileBrowserEntryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub selectable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FileBrowserEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreateFileReferenceParams {
    pub task_id: TaskId,
    pub entry_id: FileBrowserEntryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreateFileReferenceResult {
    pub attachment: PreSendAttachment,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreatePastedImageParams {
    pub task_id: TaskId,
    pub label: String,
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreatePastedImageResult {
    pub attachment: PreSendAttachment,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreateEmbeddedCandidateParams {
    pub task_id: TaskId,
    pub entry_id: FileBrowserEntryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreateEmbeddedCandidateResult {
    pub candidate: EmbeddedAttachmentCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentConfirmEmbeddedParams {
    pub task_id: TaskId,
    pub candidates: Vec<AttachmentCandidateId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentConfirmEmbeddedResult {
    pub attachments: Vec<PreSendAttachment>,
    pub errors: Vec<AttachmentCandidateError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRefreshHandlesParams {
    pub task_id: TaskId,
    pub handles: Vec<AttachmentHandleId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRefreshHandlesResult {
    pub attachments: Vec<PreSendAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentReleaseHandlesParams {
    pub task_id: TaskId,
    pub handles: Vec<AttachmentHandleId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentReleaseHandlesResult {
    pub released_handles: Vec<AttachmentHandleId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRevealParams {
    pub task_id: TaskId,
    pub handle_id: AttachmentHandleId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRevealResult {
    pub requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PreSendAttachment {
    pub handle_id: AttachmentHandleId,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedAttachmentCandidate {
    pub candidate_id: AttachmentCandidateId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCandidateError {
    pub candidate_id: AttachmentCandidateId,
    pub code: AttachmentCandidateErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentCandidateErrorCode {
    UnknownCandidate,
    WrongTask,
    NotText,
    TooLarge,
    ReadFailed,
}

#[cfg(test)]
mod tests;
