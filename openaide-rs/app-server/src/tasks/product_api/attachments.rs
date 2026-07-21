use openaide_app_server_protocol::attachment::{
    AttachmentConfirmEmbeddedParams, AttachmentConfirmEmbeddedResult,
    AttachmentCreateEmbeddedCandidateParams, AttachmentCreateEmbeddedCandidateResult,
    AttachmentCreateFileReferenceParams, AttachmentCreateFileReferenceResult,
    AttachmentCreateLocalFileReferencesParams, AttachmentCreateLocalFileReferencesResult,
    AttachmentCreatePastedImageParams, AttachmentCreatePastedImageResult,
    AttachmentListDirectoryParams, AttachmentListDirectoryResult, AttachmentListRootsParams,
    AttachmentListRootsResult, AttachmentRefreshHandlesParams, AttachmentRefreshHandlesResult,
    AttachmentReleaseParams, AttachmentReleaseResult, AttachmentResourceId, AttachmentRevealParams,
    PreSendAttachment,
};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::workspace::{
    WorkspaceBrowserDirectory, WorkspaceBrowserEntry, WorkspaceBrowserRoot,
    WorkspaceListDirectoryParams, WorkspaceListDirectoryResult, WorkspaceListRootsParams,
    WorkspaceListRootsResult,
};
use std::path::{Path, PathBuf};

use crate::attachment_runtime::{
    AttachmentOwner, AttachmentRuntimeError, ResolvedRevealAttachment,
};
use crate::protocol::model::NormalizedMessage;

use super::{validation_error, TaskProductApi};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedSentFile {
    pub path: PathBuf,
    pub label: String,
}

pub(crate) trait AttachmentFileBrowserWorkflow: Send + Sync {
    fn keep_alive_for_client(&self, client_instance_id: &ClientInstanceId);
    fn discard_resources_for_client(&self, client_instance_id: &ClientInstanceId);

    fn resolve_sent_file(
        &self,
        _client_instance_id: &ClientInstanceId,
        _task_id: &TaskId,
        _message_id: &str,
        _attachment_index: usize,
    ) -> Result<ResolvedSentFile, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "Sent file access is unavailable".to_string(),
            recoverable: true,
            target: None,
        })
    }

    fn list_roots(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentListRootsParams,
    ) -> Result<AttachmentListRootsResult, ProtocolError>;

    fn list_directory(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentListDirectoryParams,
    ) -> Result<AttachmentListDirectoryResult, ProtocolError>;

    fn create_file_reference(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentCreateFileReferenceParams,
    ) -> Result<AttachmentCreateFileReferenceResult, ProtocolError>;

    fn create_local_file_references(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: AttachmentCreateLocalFileReferencesParams,
    ) -> Result<AttachmentCreateLocalFileReferencesResult, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "local file references are unavailable".to_string(),
            recoverable: true,
            target: None,
        })
    }

    /// Registers one completed Web upload without exposing its temp path to Frontend code.
    fn create_uploaded_file_reference(
        &self,
        _client_instance_id: &ClientInstanceId,
        _task_id: &TaskId,
        _path: String,
        _label: String,
    ) -> Result<PreSendAttachment, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "Web uploads are unavailable".to_string(),
            recoverable: true,
            target: None,
        })
    }

    fn create_pasted_image(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentCreatePastedImageParams,
    ) -> Result<AttachmentCreatePastedImageResult, ProtocolError>;

    fn create_embedded_candidate(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentCreateEmbeddedCandidateParams,
    ) -> Result<AttachmentCreateEmbeddedCandidateResult, ProtocolError>;

    fn confirm_embedded(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentConfirmEmbeddedParams,
    ) -> Result<AttachmentConfirmEmbeddedResult, ProtocolError>;

    fn refresh_handles(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentRefreshHandlesParams,
    ) -> Result<AttachmentRefreshHandlesResult, ProtocolError>;

    fn release_resources(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentReleaseParams,
    ) -> Result<AttachmentReleaseResult, ProtocolError>;

    fn resolve_reveal_target(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentRevealParams,
    ) -> Result<ResolvedRevealAttachment, ProtocolError>;

    fn workspace_roots(
        &self,
        params: WorkspaceListRootsParams,
    ) -> Result<WorkspaceListRootsResult, ProtocolError>;

    fn workspace_directory(
        &self,
        params: WorkspaceListDirectoryParams,
    ) -> Result<WorkspaceListDirectoryResult, ProtocolError>;
}

impl AttachmentFileBrowserWorkflow for TaskProductApi {
    fn keep_alive_for_client(&self, client_instance_id: &ClientInstanceId) {
        self.attachments.keep_alive_for_client(client_instance_id);
    }

    fn discard_resources_for_client(&self, client_instance_id: &ClientInstanceId) {
        self.attachments
            .discard_resources_for_client(client_instance_id);
    }

    fn resolve_sent_file(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        message_id: &str,
        attachment_index: usize,
    ) -> Result<ResolvedSentFile, ProtocolError> {
        self.read_task_for_client(task_id.as_str(), client_instance_id)?;
        let messages = self
            .store
            .read_messages(task_id.as_str())
            .map_err(super::runtime_error)?;
        let attachment = messages
            .iter()
            .find(|stored| stored.chat.message_id == message_id)
            .and_then(|stored| match &stored.chat.message {
                NormalizedMessage::User { attachments, .. } => attachments.get(attachment_index),
                _ => None,
            })
            .ok_or_else(|| sent_file_not_found("Sent attachment was not found"))?;
        let path = attachment
            .path
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .ok_or_else(|| sent_file_not_found("Sent file is no longer available"))?;
        Ok(ResolvedSentFile {
            path,
            label: attachment.label.clone(),
        })
    }

    fn list_roots(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentListRootsParams,
    ) -> Result<AttachmentListRootsResult, ProtocolError> {
        let task = self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        Ok(self
            .attachments
            .list_roots(&params.task_id, task.workspace_root))
    }

    fn list_directory(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentListDirectoryParams,
    ) -> Result<AttachmentListDirectoryResult, ProtocolError> {
        let task = self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        self.attachments
            .list_directory(
                &owner,
                task.workspace_root,
                &params.root_id,
                params.directory_id.as_ref(),
            )
            .map_err(protocol_error_from_attachment_runtime)
    }

    fn create_file_reference(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentCreateFileReferenceParams,
    ) -> Result<AttachmentCreateFileReferenceResult, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        self.attachments
            .create_file_reference(&owner, &params.entry_id)
            .map_err(protocol_error_from_attachment_runtime)
    }

    fn create_local_file_references(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentCreateLocalFileReferencesParams,
    ) -> Result<AttachmentCreateLocalFileReferencesResult, ProtocolError> {
        const MAX_FILES: usize = 20;
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        if params.paths.len() > MAX_FILES {
            return Err(validation_error(
                "paths",
                "A draft can attach at most 20 files",
            ));
        }
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        let mut attachments = Vec::with_capacity(params.paths.len());
        for path in params.paths {
            match self
                .attachments
                .create_local_file_reference(&owner, path, None)
            {
                Ok(attachment) => attachments.push(attachment),
                Err(error) => {
                    let resources = attachments
                        .iter()
                        .map(|attachment| AttachmentResourceId::Handle {
                            id: attachment.handle_id.clone(),
                        })
                        .collect::<Vec<_>>();
                    self.attachments.release_resources(&owner, &resources);
                    return Err(protocol_error_from_attachment_runtime(error));
                }
            }
        }
        Ok(AttachmentCreateLocalFileReferencesResult { attachments })
    }

    fn create_uploaded_file_reference(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        path: String,
        label: String,
    ) -> Result<PreSendAttachment, ProtocolError> {
        self.read_task_for_client(task_id.as_str(), client_instance_id)?;
        self.attachments
            .create_local_file_reference(
                AttachmentOwner::new(client_instance_id, task_id),
                path,
                Some(label),
            )
            .map_err(protocol_error_from_attachment_runtime)
    }

    fn create_pasted_image(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentCreatePastedImageParams,
    ) -> Result<AttachmentCreatePastedImageResult, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        self.attachments
            .create_pasted_image(&owner, params.label, params.mime_type, params.data)
            .map_err(protocol_error_from_attachment_runtime)
    }

    fn create_embedded_candidate(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentCreateEmbeddedCandidateParams,
    ) -> Result<AttachmentCreateEmbeddedCandidateResult, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        self.attachments
            .create_embedded_candidate(&owner, &params.entry_id)
            .map_err(protocol_error_from_attachment_runtime)
    }

    fn confirm_embedded(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentConfirmEmbeddedParams,
    ) -> Result<AttachmentConfirmEmbeddedResult, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        Ok(self
            .attachments
            .confirm_embedded(&owner, &params.candidates))
    }

    fn refresh_handles(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentRefreshHandlesParams,
    ) -> Result<AttachmentRefreshHandlesResult, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        self.attachments
            .refresh_handles(&owner, &params.handles)
            .map_err(protocol_error_from_attachment_runtime)
    }

    fn release_resources(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentReleaseParams,
    ) -> Result<AttachmentReleaseResult, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        Ok(self
            .attachments
            .release_resources(&owner, &params.resources))
    }

    fn resolve_reveal_target(
        &self,
        client_instance_id: &ClientInstanceId,
        params: AttachmentRevealParams,
    ) -> Result<ResolvedRevealAttachment, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        self.attachments
            .resolve_for_reveal(&owner, &params.handle_id)
            .map_err(protocol_error_from_attachment_runtime)
    }

    fn workspace_roots(
        &self,
        _params: WorkspaceListRootsParams,
    ) -> Result<WorkspaceListRootsResult, ProtocolError> {
        Ok(WorkspaceListRootsResult {
            roots: workspace_root_candidates(),
        })
    }

    fn workspace_directory(
        &self,
        params: WorkspaceListDirectoryParams,
    ) -> Result<WorkspaceListDirectoryResult, ProtocolError> {
        workspace_directory_listing(PathBuf::from(params.path))
    }
}

fn workspace_root_candidates() -> Vec<WorkspaceBrowserRoot> {
    let mut paths = Vec::new();
    if let Ok(current) = std::env::current_dir() {
        paths.push(current);
    }
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home));
    }
    #[cfg(unix)]
    paths.push(PathBuf::from("/"));

    let mut roots = Vec::new();
    for path in paths {
        let path = normalize_workspace_browser_path(path);
        if roots
            .iter()
            .any(|root: &WorkspaceBrowserRoot| root.path == path)
        {
            continue;
        }
        roots.push(WorkspaceBrowserRoot {
            label: workspace_browser_label(Path::new(&path)),
            path,
        });
    }
    roots
}

fn workspace_directory_listing(
    path: PathBuf,
) -> Result<WorkspaceListDirectoryResult, ProtocolError> {
    let path = normalize_workspace_browser_path(path);
    let directory_path = PathBuf::from(&path);
    let read_dir = std::fs::read_dir(&directory_path).map_err(workspace_browser_read_error)?;
    let mut entries = Vec::new();
    for item in read_dir {
        let item = item.map_err(workspace_browser_read_error)?;
        let metadata = item.metadata().map_err(workspace_browser_read_error)?;
        if !metadata.is_dir() {
            continue;
        }
        let label = item.file_name().to_string_lossy().to_string();
        if is_hidden_workspace_browser_directory(&label) {
            continue;
        }
        entries.push(WorkspaceBrowserEntry {
            label,
            path: normalize_workspace_browser_path(item.path()),
        });
    }
    entries.sort_by(|left, right| {
        left.label
            .to_lowercase()
            .cmp(&right.label.to_lowercase())
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(WorkspaceListDirectoryResult {
        directory: WorkspaceBrowserDirectory {
            label: workspace_browser_label(&directory_path),
            path,
            parent_path: directory_path
                .parent()
                .map(normalize_workspace_browser_path),
        },
        entries,
    })
}

fn normalize_workspace_browser_path(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().to_string()
}

fn workspace_browser_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn is_hidden_workspace_browser_directory(label: &str) -> bool {
    if label.starts_with('.') {
        return true;
    }
    matches!(
        label,
        "build" | "coverage" | "dist" | "node_modules" | "target" | "tmp"
    )
}

fn workspace_browser_read_error(error: impl std::fmt::Display) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Internal,
        message: format!("Unable to read directory: {error}"),
        recoverable: true,
        target: None,
    }
}

fn sent_file_not_found(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotFound,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}

fn protocol_error_from_attachment_runtime(error: AttachmentRuntimeError) -> ProtocolError {
    match error {
        AttachmentRuntimeError::InvalidRoot => validation_error("rootId", "Invalid file root"),
        AttachmentRuntimeError::OutsideAllowedRoot => {
            validation_error("entryId", "File entry resolves outside the allowed root")
        }
        AttachmentRuntimeError::UnknownEntry => validation_error("entryId", "Unknown file entry"),
        AttachmentRuntimeError::WrongTask => {
            validation_error("taskId", "File entry belongs to another Task")
        }
        AttachmentRuntimeError::NotDirectory => {
            validation_error("directoryId", "File entry is not a directory")
        }
        AttachmentRuntimeError::NotFile => validation_error("entryId", "File entry is not a file"),
        AttachmentRuntimeError::NotText => validation_error(
            "entryId",
            "Embedded snapshots support UTF-8 text files only",
        ),
        AttachmentRuntimeError::TooLarge => validation_error("data", "Attachment is too large"),
        AttachmentRuntimeError::InvalidImage => {
            validation_error("data", "Pasted image payload is invalid")
        }
        AttachmentRuntimeError::ReadFailed(message) => ProtocolError {
            code: ProtocolErrorCode::Internal,
            message,
            recoverable: true,
            target: None,
        },
        AttachmentRuntimeError::UnknownHandle | AttachmentRuntimeError::DuplicateHandle => {
            validation_error("attachments", "Invalid attachment handle")
        }
    }
}
