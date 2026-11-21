use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{
    AttachmentCandidateId, AttachmentHandleId, FileBrowserEntryId, FileBrowserRootId, TaskId,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreateLocalFileReferencesParams {
    pub task_id: TaskId,
    /// Trusted App Shell paths. Shared Frontend code must never populate this field.
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreateLocalFileReferencesResult {
    pub attachments: Vec<PreSendAttachment>,
}

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
pub struct AttachmentReleaseParams {
    pub task_id: TaskId,
    /// Resolver resources are released independently in this order.
    pub resources: Vec<AttachmentResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentReleaseResult {
    /// Contains exactly one outcome per requested resource, preserving request order.
    pub outcomes: Vec<AttachmentReleaseOutcome>,
}

/// Identifies one transient resolver resource without conflating handles and candidates.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AttachmentResourceId {
    Handle { id: AttachmentHandleId },
    Candidate { id: AttachmentCandidateId },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentReleaseOutcome {
    pub resource: AttachmentResourceId,
    pub status: AttachmentReleaseStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentReleaseStatus {
    Released,
    /// The resource was absent, expired, consumed, or protected by an active send reservation.
    NoOp,
    /// The resource exists but belongs to another client or Task scope.
    Forbidden,
}

/// Moves pre-Send handles from a replaced Prepared Task to its successor.
///
/// A New Task context change (Agent, Project, or worktree) swaps the Prepared Task under an
/// unsent draft. The handles keep their ids and resolved content; only their Task scope
/// changes, so the draft never uploads the same file twice. The caller needs access to
/// `to_task_id` only: the source lease is usually already released, and each handle's own
/// client-and-Task owner proves it may be moved.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentTransferParams {
    pub from_task_id: TaskId,
    pub to_task_id: TaskId,
    pub handles: Vec<AttachmentHandleId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentTransferResult {
    /// Contains exactly one outcome per requested handle, preserving request order.
    pub outcomes: Vec<AttachmentTransferOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentTransferOutcome {
    pub handle_id: AttachmentHandleId,
    pub status: AttachmentTransferStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentTransferStatus {
    /// The handle now belongs to `to_task_id`, including one that already did.
    Transferred,
    /// The handle was absent, expired, consumed, or reserved by an in-flight Send.
    Missing,
    /// The handle belongs to another client or to a Task other than `from_task_id`.
    Forbidden,
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
pub struct AttachmentRevealSentParams {
    pub task_id: TaskId,
    pub message_id: String,
    pub attachment_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRevealSentResult {
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
#[path = "attachment_tests.rs"]
mod tests;
