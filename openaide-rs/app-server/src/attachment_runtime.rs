use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Barrier;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;

use openaide_app_server_protocol::attachment::{
    AttachmentCreatePastedImageResult, PreSendAttachment,
};
use openaide_app_server_protocol::ids::{AttachmentHandleId, ClientInstanceId};
use openaide_app_server_protocol::task::ComposerImage;

use crate::protocol::model::Attachment;

mod embedded;
mod file_browser;
mod handles;
mod ownership;
mod path_validation;
mod reservation;
mod resources;

pub(crate) use ownership::AttachmentOwner;
pub(crate) use reservation::AttachmentSendReservation;

use resources::{
    safe_image_label, validate_pasted_image, AttachmentTarget, EmbeddedAttachmentCandidateHandle,
    FileBrowserEntryHandle, PreSendAttachmentHandle,
};

const ABANDONED_RESOURCE_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone)]
pub(crate) struct AttachmentRuntime {
    pub(super) state: Arc<Mutex<AttachmentRuntimeState>>,
    ttl: Duration,
}

#[derive(Debug, Default)]
pub(super) struct AttachmentRuntimeState {
    #[allow(dead_code)]
    pub(super) next_id: u64,
    pub(super) next_candidate_id: u64,
    pub(super) next_entry_id: u64,
    pub(super) handles: HashMap<String, PreSendAttachmentHandle>,
    pub(super) candidates: HashMap<String, EmbeddedAttachmentCandidateHandle>,
    pub(super) entries: HashMap<String, FileBrowserEntryHandle>,
    pub(super) reserved_handles: HashSet<String>,
    #[cfg(test)]
    embedded_confirmation_gate: Option<Arc<EmbeddedConfirmationTestGate>>,
}

#[cfg(test)]
#[derive(Debug)]
struct EmbeddedConfirmationTestGate {
    arrival: Barrier,
    continuation: Barrier,
}

#[cfg(test)]
impl EmbeddedConfirmationTestGate {
    fn new(expected_arrivals: usize) -> Self {
        assert!(expected_arrivals > 0);
        let participants = expected_arrivals + 1;
        Self {
            arrival: Barrier::new(participants),
            continuation: Barrier::new(participants),
        }
    }

    fn arrive_and_wait(&self) {
        self.arrival.wait();
        self.continuation.wait();
    }

    fn wait_until_arrived(&self) {
        self.arrival.wait();
    }

    fn release(&self) {
        self.continuation.wait();
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegisteredAttachmentHandle {
    pub(crate) handle_id: AttachmentHandleId,
    pub(crate) label: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedSendAttachments {
    chat_attachments: Vec<Attachment>,
    agent_attachments: Vec<Attachment>,
    #[cfg(test)]
    fingerprint_handles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedRevealAttachment {
    pub(crate) path: PathBuf,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttachmentRuntimeError {
    UnknownHandle,
    WrongTask,
    DuplicateHandle,
    InvalidRoot,
    OutsideAllowedRoot,
    UnknownEntry,
    NotDirectory,
    NotFile,
    NotText,
    TooLarge,
    InvalidImage,
    ReadFailed(String),
}

impl AttachmentRuntime {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(AttachmentRuntimeState::default())),
            ttl: ABANDONED_RESOURCE_TTL,
        }
    }

    #[cfg(test)]
    pub(crate) fn expire_all_for_test(&self) {
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        let expired = Instant::now() - Duration::from_secs(1);
        for handle in state.handles.values_mut() {
            handle.expires_at = expired;
        }
        for candidate in state.candidates.values_mut() {
            candidate.expires_at = expired;
        }
        for entry in state.entries.values_mut() {
            entry.expires_at = expired;
        }
    }

    #[cfg(test)]
    fn pause_embedded_confirmations_for_test(
        &self,
        expected_arrivals: usize,
    ) -> Arc<EmbeddedConfirmationTestGate> {
        let gate = Arc::new(EmbeddedConfirmationTestGate::new(expected_arrivals));
        self.state
            .lock()
            .expect("attachment runtime mutex poisoned")
            .embedded_confirmation_gate = Some(Arc::clone(&gate));
        gate
    }

    #[cfg(test)]
    fn pause_after_embedded_candidate_lookup_for_test(&self) {
        let gate = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned")
            .embedded_confirmation_gate
            .clone();
        if let Some(gate) = gate {
            gate.arrive_and_wait();
        }
    }

    #[cfg(test)]
    pub(crate) fn expire_all_at_test_deadline(&self) -> Instant {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        for handle in state.handles.values_mut() {
            handle.expires_at = deadline;
        }
        for candidate in state.candidates.values_mut() {
            candidate.expires_at = deadline;
        }
        for entry in state.entries.values_mut() {
            entry.expires_at = deadline;
        }
        deadline
    }

    #[cfg(test)]
    pub(crate) fn prune_expired_at_for_test(&self, now: Instant) {
        self.state
            .lock()
            .expect("attachment runtime mutex poisoned")
            .prune_expired(now);
    }

    #[cfg(test)]
    pub(crate) fn register_file_reference_for_test(
        &self,
        owner: impl Into<AttachmentOwner>,
        label: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> RegisteredAttachmentHandle {
        let path = path.into();
        let allowed_root = path_validation::AllowedRoot::new(
            path.parent().expect("test file reference has a parent"),
        )
        .expect("test file reference root is valid");
        self.register_file_reference(owner, label, path, allowed_root)
    }

    #[allow(dead_code)]
    pub(super) fn register_file_reference(
        &self,
        owner: impl Into<AttachmentOwner>,
        label: impl Into<String>,
        path: impl Into<PathBuf>,
        allowed_root: path_validation::AllowedRoot,
    ) -> RegisteredAttachmentHandle {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        state.next_id += 1;
        let handle_id = AttachmentHandleId::from(format!("attachment-handle-{}", state.next_id));
        let label = label.into();
        state.handles.insert(
            handle_id.as_str().to_string(),
            PreSendAttachmentHandle {
                owner,
                label: label.clone(),
                target: AttachmentTarget::FileReference {
                    path: path.into(),
                    allowed_root,
                },
                expires_at: self.expires_at(),
            },
        );
        RegisteredAttachmentHandle { handle_id, label }
    }

    pub(crate) fn create_pasted_image(
        &self,
        owner: impl Into<AttachmentOwner>,
        label: impl Into<String>,
        mime_type: impl Into<String>,
        data: impl Into<String>,
    ) -> Result<AttachmentCreatePastedImageResult, AttachmentRuntimeError> {
        let label = safe_image_label(label.into());
        let mime_type = mime_type.into();
        let data = data.into();
        let size_bytes = validate_pasted_image(&mime_type, &data)?;
        let registered = self.register_pasted_image(owner, label, mime_type, data, size_bytes);
        Ok(AttachmentCreatePastedImageResult {
            attachment: PreSendAttachment {
                handle_id: registered.handle_id,
                label: registered.label,
            },
        })
    }

    /// Converts a completed binary Web upload into the existing image handle model.
    pub(crate) fn create_uploaded_image(
        &self,
        owner: impl Into<AttachmentOwner>,
        path: impl AsRef<std::path::Path>,
        label: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Result<AttachmentCreatePastedImageResult, AttachmentRuntimeError> {
        let bytes = std::fs::read(path)
            .map_err(|error| AttachmentRuntimeError::ReadFailed(error.to_string()))?;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        self.create_pasted_image(owner, label, mime_type, data)
    }

    /// Registers an exact user-selected local file without granting directory browsing.
    pub(crate) fn create_local_file_reference(
        &self,
        owner: impl Into<AttachmentOwner>,
        path: impl Into<PathBuf>,
        label: Option<String>,
    ) -> Result<PreSendAttachment, AttachmentRuntimeError> {
        let path = std::fs::canonicalize(path.into())
            .map_err(|error| AttachmentRuntimeError::ReadFailed(error.to_string()))?;
        if !path.is_file() {
            return Err(AttachmentRuntimeError::NotFile);
        }
        let label = label
            .as_deref()
            .or_else(|| path.file_name().and_then(|value| value.to_str()))
            .filter(|value| !value.is_empty())
            .unwrap_or("Attached file")
            .chars()
            .take(160)
            .collect::<String>();
        let allowed_root = path_validation::AllowedRoot::new(
            path.parent().ok_or(AttachmentRuntimeError::InvalidRoot)?,
        )?;
        let registered = self.register_file_reference(owner, label, path, allowed_root);
        Ok(PreSendAttachment {
            handle_id: registered.handle_id,
            label: registered.label,
        })
    }

    fn register_pasted_image(
        &self,
        owner: impl Into<AttachmentOwner>,
        label: String,
        mime_type: String,
        data: String,
        size_bytes: u64,
    ) -> RegisteredAttachmentHandle {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        state.next_id += 1;
        let handle_id = AttachmentHandleId::from(format!("attachment-handle-{}", state.next_id));
        state.handles.insert(
            handle_id.as_str().to_string(),
            PreSendAttachmentHandle {
                owner,
                label: label.clone(),
                target: AttachmentTarget::PastedImage {
                    mime_type,
                    data,
                    size_bytes,
                },
                expires_at: self.expires_at(),
            },
        );
        RegisteredAttachmentHandle { handle_id, label }
    }

    pub(super) fn expires_at(&self) -> Instant {
        Instant::now() + self.ttl
    }

    pub(crate) fn keep_alive_for_client(&self, client_instance_id: &ClientInstanceId) {
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        let now = Instant::now();
        let expires_at = now + self.ttl;
        for handle in state.handles.values_mut() {
            if handle.owner.belongs_to_client(client_instance_id) {
                handle.expires_at = expires_at;
            }
        }
        for candidate in state.candidates.values_mut() {
            if candidate.owner.belongs_to_client(client_instance_id) {
                candidate.expires_at = expires_at;
            }
        }
        for entry in state.entries.values_mut() {
            if entry.owner.belongs_to_client(client_instance_id) {
                entry.expires_at = expires_at;
            }
        }
        state.prune_expired(now);
    }

    pub(crate) fn discard_resources_for_client(&self, client_instance_id: &ClientInstanceId) {
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state
            .handles
            .retain(|_, handle| !handle.owner.belongs_to_client(client_instance_id));
        state
            .candidates
            .retain(|_, candidate| !candidate.owner.belongs_to_client(client_instance_id));
        state
            .entries
            .retain(|_, entry| !entry.owner.belongs_to_client(client_instance_id));
        state.remove_orphaned_reservations();
    }

    pub(crate) fn discard_resources_for_task(
        &self,
        task_id: &openaide_app_server_protocol::ids::TaskId,
    ) {
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state
            .handles
            .retain(|_, handle| !handle.owner.belongs_to_task_id(task_id));
        state
            .candidates
            .retain(|_, candidate| !candidate.owner.belongs_to_task_id(task_id));
        state
            .entries
            .retain(|_, entry| !entry.owner.belongs_to_task_id(task_id));
        state.remove_orphaned_reservations();
    }
}

impl Default for AttachmentRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolvedSendAttachments {
    /// Validates and materializes client-owned Images at the Send boundary.
    pub(crate) fn from_inline_images(
        images: &[ComposerImage],
    ) -> Result<Self, AttachmentRuntimeError> {
        const IMAGE_MAX_BYTES: usize = 5 * 1024 * 1024;
        const MESSAGE_IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;

        let mut total = 0usize;
        let mut attachments = Vec::with_capacity(images.len());
        for image in images {
            let size =
                crate::media::validate_base64_image(&image.mime_type, &image.data, IMAGE_MAX_BYTES)
                    .map_err(|error| match error {
                        crate::media::MediaDataError::Invalid => {
                            AttachmentRuntimeError::InvalidImage
                        }
                        crate::media::MediaDataError::TooLarge => AttachmentRuntimeError::TooLarge,
                    })?;
            total = total
                .checked_add(size)
                .filter(|total| *total <= MESSAGE_IMAGE_MAX_BYTES)
                .ok_or(AttachmentRuntimeError::TooLarge)?;
            attachments.push(Attachment {
                kind: "image".to_string(),
                label: resources::safe_image_label(image.label.clone()),
                path: None,
                payload: Some(serde_json::json!({
                    "data": image.data,
                    "mimeType": image.mime_type,
                    "sizeBytes": size,
                })),
            });
        }
        Ok(Self {
            chat_attachments: attachments.clone(),
            agent_attachments: attachments,
            #[cfg(test)]
            fingerprint_handles: Vec::new(),
        })
    }

    /// Adds Frontend-owned inline Images after App Server-owned file resources.
    pub(crate) fn clone_with_inline_images(
        &self,
        images: &[ComposerImage],
    ) -> Result<Self, AttachmentRuntimeError> {
        let inline_images = Self::from_inline_images(images)?;
        let mut merged = self.clone();
        merged
            .chat_attachments
            .extend(inline_images.chat_attachments);
        merged
            .agent_attachments
            .extend(inline_images.agent_attachments);
        Ok(merged)
    }

    pub(crate) fn chat_attachments(&self) -> Vec<Attachment> {
        self.chat_attachments.clone()
    }

    pub(crate) fn agent_attachments(&self) -> Vec<Attachment> {
        self.agent_attachments.clone()
    }

    #[cfg(test)]
    pub(crate) fn fingerprint_handles(&self) -> Vec<String> {
        self.fingerprint_handles.clone()
    }
}

impl AttachmentRuntimeState {
    fn prune_expired(&mut self, now: Instant) {
        self.handles.retain(|handle_id, handle| {
            handle.expires_at > now || self.reserved_handles.contains(handle_id)
        });
        self.candidates
            .retain(|_, candidate| candidate.expires_at > now);
        self.entries.retain(|_, entry| entry.expires_at > now);
        self.remove_orphaned_reservations();
    }

    fn remove_orphaned_reservations(&mut self) {
        self.reserved_handles
            .retain(|handle_id| self.handles.contains_key(handle_id));
    }
}

#[cfg(test)]
mod tests;
