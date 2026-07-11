use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use openaide_app_server_protocol::attachment::{
    AttachmentCreatePastedImageResult, PreSendAttachment,
};
use openaide_app_server_protocol::ids::{AttachmentHandleId, ClientInstanceId};

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
}

impl Default for AttachmentRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolvedSendAttachments {
    pub(crate) fn chat_attachments(&self) -> Vec<Attachment> {
        self.chat_attachments.clone()
    }

    pub(crate) fn agent_attachments(&self) -> Vec<Attachment> {
        self.agent_attachments.clone()
    }

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
