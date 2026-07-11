use std::collections::BTreeSet;
use std::time::Instant;

use openaide_app_server_protocol::attachment::{
    AttachmentCandidateError, AttachmentCandidateErrorCode, AttachmentConfirmEmbeddedResult,
    AttachmentCreateEmbeddedCandidateResult, EmbeddedAttachmentCandidate, FileBrowserEntryKind,
    PreSendAttachment,
};
use openaide_app_server_protocol::ids::AttachmentCandidateId;

use super::{
    AttachmentOwner, AttachmentRuntime, AttachmentRuntimeError, AttachmentTarget,
    EmbeddedAttachmentCandidateHandle, PreSendAttachmentHandle,
};

pub(super) const EMBEDDED_TEXT_MAX_BYTES: usize = 256 * 1024;

impl AttachmentRuntime {
    pub(crate) fn create_embedded_candidate(
        &self,
        owner: impl Into<AttachmentOwner>,
        entry_id: &openaide_app_server_protocol::ids::FileBrowserEntryId,
    ) -> Result<AttachmentCreateEmbeddedCandidateResult, AttachmentRuntimeError> {
        let owner = owner.into();
        let entry = {
            let mut state = self
                .state
                .lock()
                .expect("attachment runtime mutex poisoned");
            state.prune_expired(Instant::now());
            state
                .entries
                .get(entry_id.as_str())
                .cloned()
                .ok_or(AttachmentRuntimeError::UnknownEntry)?
        };
        if !entry.owner.belongs_to(&owner) {
            return Err(if entry.owner.belongs_to_task(&owner) {
                AttachmentRuntimeError::UnknownEntry
            } else {
                AttachmentRuntimeError::WrongTask
            });
        }
        if entry.kind != FileBrowserEntryKind::File {
            return Err(AttachmentRuntimeError::NotFile);
        }
        entry.allowed_root.validate_file(&entry.path)?;
        let size_bytes = validate_embedded_file(&entry.path)?;
        let candidate = self.register_embedded_candidate(
            &owner,
            entry.label,
            entry.path,
            entry.allowed_root,
            Some(size_bytes),
        );
        Ok(AttachmentCreateEmbeddedCandidateResult { candidate })
    }

    pub(crate) fn confirm_embedded(
        &self,
        owner: impl Into<AttachmentOwner>,
        candidates: &[AttachmentCandidateId],
    ) -> AttachmentConfirmEmbeddedResult {
        let owner = owner.into();
        let mut seen = BTreeSet::new();
        let mut attachments = Vec::new();
        let mut errors = Vec::new();

        for candidate_id in candidates {
            if !seen.insert(candidate_id.as_str().to_string()) {
                errors.push(candidate_error(
                    candidate_id.clone(),
                    AttachmentCandidateErrorCode::UnknownCandidate,
                    "Duplicate embedded candidate",
                ));
                continue;
            }
            match self.confirm_one_embedded(&owner, candidate_id) {
                Ok(attachment) => attachments.push(attachment),
                Err(error) => errors.push(error),
            }
        }

        AttachmentConfirmEmbeddedResult {
            attachments,
            errors,
        }
    }

    fn register_embedded_candidate(
        &self,
        owner: impl Into<AttachmentOwner>,
        label: String,
        path: std::path::PathBuf,
        allowed_root: super::path_validation::AllowedRoot,
        size_bytes: Option<u64>,
    ) -> EmbeddedAttachmentCandidate {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        state.next_candidate_id += 1;
        let candidate_id = AttachmentCandidateId::from(format!(
            "attachment-candidate-{}",
            state.next_candidate_id
        ));
        state.candidates.insert(
            candidate_id.as_str().to_string(),
            EmbeddedAttachmentCandidateHandle {
                owner,
                label: label.clone(),
                path,
                allowed_root,
                size_bytes,
                expires_at: self.expires_at(),
            },
        );
        EmbeddedAttachmentCandidate {
            candidate_id,
            label,
            size_bytes,
        }
    }

    fn confirm_one_embedded(
        &self,
        owner: &AttachmentOwner,
        candidate_id: &AttachmentCandidateId,
    ) -> Result<PreSendAttachment, AttachmentCandidateError> {
        let candidate = {
            let mut state = self
                .state
                .lock()
                .expect("attachment runtime mutex poisoned");
            state.prune_expired(Instant::now());
            match state.candidates.get(candidate_id.as_str()) {
                Some(candidate) => candidate.clone(),
                None => {
                    return Err(candidate_error(
                        candidate_id.clone(),
                        AttachmentCandidateErrorCode::UnknownCandidate,
                        "Unknown embedded candidate",
                    ))
                }
            }
        };

        if !candidate.owner.belongs_to(owner) {
            let (code, message) = if candidate.owner.belongs_to_task(owner) {
                (
                    AttachmentCandidateErrorCode::UnknownCandidate,
                    "Unknown embedded candidate",
                )
            } else {
                (
                    AttachmentCandidateErrorCode::WrongTask,
                    "Embedded candidate belongs to another Task",
                )
            };
            return Err(candidate_error(candidate_id.clone(), code, message));
        }
        if let Err(error) = candidate.allowed_root.validate_file(&candidate.path) {
            return Err(candidate_error_from_runtime(candidate_id.clone(), error));
        }
        if let Err(error) = validate_embedded_file(&candidate.path) {
            return Err(candidate_error_from_runtime(candidate_id.clone(), error));
        }

        self.remove_embedded_candidate(candidate_id);
        let registered = self.register_embedded_handle(candidate);
        Ok(PreSendAttachment {
            handle_id: registered.handle_id,
            label: registered.label,
        })
    }

    fn remove_embedded_candidate(&self, candidate_id: &AttachmentCandidateId) {
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        state.candidates.remove(candidate_id.as_str());
    }

    fn register_embedded_handle(
        &self,
        candidate: EmbeddedAttachmentCandidateHandle,
    ) -> crate::attachment_runtime::RegisteredAttachmentHandle {
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        state.next_id += 1;
        let handle_id = openaide_app_server_protocol::ids::AttachmentHandleId::from(format!(
            "attachment-handle-{}",
            state.next_id
        ));
        state.handles.insert(
            handle_id.as_str().to_string(),
            PreSendAttachmentHandle {
                owner: candidate.owner,
                label: candidate.label.clone(),
                target: AttachmentTarget::EmbeddedSnapshot {
                    path: candidate.path,
                    allowed_root: candidate.allowed_root,
                },
                expires_at: self.expires_at(),
            },
        );
        crate::attachment_runtime::RegisteredAttachmentHandle {
            handle_id,
            label: candidate.label,
        }
    }
}

fn validate_embedded_file(path: &std::path::Path) -> Result<u64, AttachmentRuntimeError> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| AttachmentRuntimeError::ReadFailed(error.to_string()))?;
    if !metadata.is_file() {
        return Err(AttachmentRuntimeError::NotFile);
    }
    let len = metadata.len();
    if len > EMBEDDED_TEXT_MAX_BYTES as u64 {
        return Err(AttachmentRuntimeError::TooLarge);
    }
    if !is_utf8(path)? {
        return Err(AttachmentRuntimeError::NotText);
    }
    Ok(len)
}

fn is_utf8(path: &std::path::Path) -> Result<bool, AttachmentRuntimeError> {
    match std::fs::read_to_string(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => Ok(false),
        Err(error) => Err(AttachmentRuntimeError::ReadFailed(error.to_string())),
    }
}

fn candidate_error_from_runtime(
    candidate_id: AttachmentCandidateId,
    error: AttachmentRuntimeError,
) -> AttachmentCandidateError {
    match error {
        AttachmentRuntimeError::NotText => candidate_error(
            candidate_id,
            AttachmentCandidateErrorCode::NotText,
            "Embedded snapshots support UTF-8 text files only",
        ),
        AttachmentRuntimeError::TooLarge => candidate_error(
            candidate_id,
            AttachmentCandidateErrorCode::TooLarge,
            "Embedded snapshot is too large",
        ),
        AttachmentRuntimeError::ReadFailed(message) => AttachmentCandidateError {
            candidate_id,
            code: AttachmentCandidateErrorCode::ReadFailed,
            message,
        },
        _ => candidate_error(
            candidate_id,
            AttachmentCandidateErrorCode::ReadFailed,
            "Embedded candidate could not be confirmed",
        ),
    }
}

fn candidate_error(
    candidate_id: AttachmentCandidateId,
    code: AttachmentCandidateErrorCode,
    message: impl Into<String>,
) -> AttachmentCandidateError {
    AttachmentCandidateError {
        candidate_id,
        code,
        message: message.into(),
    }
}
