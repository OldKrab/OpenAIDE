use std::collections::BTreeSet;
use std::time::Instant;

use openaide_app_server_protocol::attachment::{
    AttachmentRefreshHandlesResult, AttachmentReleaseOutcome, AttachmentReleaseResult,
    AttachmentReleaseStatus, AttachmentResourceId, PreSendAttachment,
};
use openaide_app_server_protocol::ids::AttachmentHandleId;

use super::{
    AttachmentOwner, AttachmentRuntime, AttachmentRuntimeError, AttachmentTarget,
    ResolvedRevealAttachment,
};
use super::{AttachmentSendReservation, ResolvedSendAttachments};

impl AttachmentRuntime {
    pub(crate) fn refresh_handles(
        &self,
        owner: impl Into<AttachmentOwner>,
        handles: &[AttachmentHandleId],
    ) -> Result<AttachmentRefreshHandlesResult, AttachmentRuntimeError> {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        let mut seen = BTreeSet::new();
        let mut attachments = Vec::with_capacity(handles.len());
        for handle_id in handles {
            if !seen.insert(handle_id.as_str().to_string()) {
                return Err(AttachmentRuntimeError::DuplicateHandle);
            }
            let handle = state
                .handles
                .get(handle_id.as_str())
                .ok_or(AttachmentRuntimeError::UnknownHandle)?;
            if state.reserved_handles.contains(handle_id.as_str()) {
                return Err(AttachmentRuntimeError::UnknownHandle);
            }
            if !handle.owner.belongs_to(&owner) {
                return Err(handle_access_error(&handle.owner, &owner));
            }
            attachments.push(PreSendAttachment {
                handle_id: handle_id.clone(),
                label: handle.label.clone(),
            });
        }
        let expires_at = self.expires_at();
        for handle_id in handles {
            state
                .handles
                .get_mut(handle_id.as_str())
                .expect("validated attachment handle remains registered")
                .expires_at = expires_at;
        }
        Ok(AttachmentRefreshHandlesResult { attachments })
    }

    pub(crate) fn release_resources(
        &self,
        owner: impl Into<AttachmentOwner>,
        resources: &[AttachmentResourceId],
    ) -> AttachmentReleaseResult {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        // Sequential outcomes make duplicate ids deterministic while one resource never blocks
        // cleanup of the rest of the batch.
        let outcomes = resources
            .iter()
            .map(|resource| {
                let status = match resource {
                    AttachmentResourceId::Handle { id } => match state.handles.get(id.as_str()) {
                        None => AttachmentReleaseStatus::NoOp,
                        Some(handle) if !handle.owner.belongs_to(&owner) => {
                            AttachmentReleaseStatus::Forbidden
                        }
                        Some(_) if state.reserved_handles.contains(id.as_str()) => {
                            AttachmentReleaseStatus::NoOp
                        }
                        Some(_) => {
                            state.handles.remove(id.as_str());
                            AttachmentReleaseStatus::Released
                        }
                    },
                    AttachmentResourceId::Candidate { id } => {
                        match state.candidates.get(id.as_str()) {
                            None => AttachmentReleaseStatus::NoOp,
                            Some(candidate) if !candidate.owner.belongs_to(&owner) => {
                                AttachmentReleaseStatus::Forbidden
                            }
                            Some(_) => {
                                state.candidates.remove(id.as_str());
                                AttachmentReleaseStatus::Released
                            }
                        }
                    }
                };
                AttachmentReleaseOutcome {
                    resource: resource.clone(),
                    status,
                }
            })
            .collect();
        AttachmentReleaseResult { outcomes }
    }

    #[cfg(test)]
    pub(crate) fn consume_handles(
        &self,
        owner: impl Into<AttachmentOwner>,
        handles: &[AttachmentHandleId],
    ) -> Result<(), AttachmentRuntimeError> {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        let mut seen = BTreeSet::new();
        for handle_id in handles {
            if !seen.insert(handle_id.as_str().to_string()) {
                return Err(AttachmentRuntimeError::DuplicateHandle);
            }
            let handle = state
                .handles
                .get(handle_id.as_str())
                .ok_or(AttachmentRuntimeError::UnknownHandle)?;
            if state.reserved_handles.contains(handle_id.as_str()) {
                return Err(AttachmentRuntimeError::UnknownHandle);
            }
            if !handle.owner.belongs_to(&owner) {
                return Err(handle_access_error(&handle.owner, &owner));
            }
        }
        for handle_id in handles {
            state.handles.remove(handle_id.as_str());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn resolve_for_send(
        &self,
        owner: impl Into<AttachmentOwner>,
        handles: &[AttachmentHandleId],
    ) -> Result<ResolvedSendAttachments, AttachmentRuntimeError> {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        let mut seen = BTreeSet::new();
        let mut chat_attachments = Vec::with_capacity(handles.len());
        let mut agent_attachments = Vec::with_capacity(handles.len());
        let mut fingerprint_handles = Vec::with_capacity(handles.len());

        for handle_id in handles {
            let id = handle_id.as_str();
            if !seen.insert(id.to_string()) {
                return Err(AttachmentRuntimeError::DuplicateHandle);
            }
            let handle = state
                .handles
                .get(id)
                .ok_or(AttachmentRuntimeError::UnknownHandle)?;
            if state.reserved_handles.contains(id) {
                return Err(AttachmentRuntimeError::UnknownHandle);
            }
            if !handle.owner.belongs_to(&owner) {
                return Err(handle_access_error(&handle.owner, &owner));
            }
            fingerprint_handles.push(id.to_string());
            chat_attachments.push(handle.chat_attachment());
            agent_attachments.push(handle.agent_attachment()?);
        }

        Ok(ResolvedSendAttachments {
            chat_attachments,
            agent_attachments,
            fingerprint_handles,
        })
    }

    pub(crate) fn reserve_for_send(
        &self,
        owner: impl Into<AttachmentOwner>,
        handles: &[AttachmentHandleId],
    ) -> Result<AttachmentSendReservation, AttachmentRuntimeError> {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        let mut seen = BTreeSet::new();
        let mut chat_attachments = Vec::with_capacity(handles.len());
        let mut agent_attachments = Vec::with_capacity(handles.len());
        let mut fingerprint_handles = Vec::with_capacity(handles.len());

        for handle_id in handles {
            let id = handle_id.as_str();
            if !seen.insert(id.to_string()) {
                return Err(AttachmentRuntimeError::DuplicateHandle);
            }
            let handle = state
                .handles
                .get(id)
                .ok_or(AttachmentRuntimeError::UnknownHandle)?;
            if state.reserved_handles.contains(id) {
                return Err(AttachmentRuntimeError::UnknownHandle);
            }
            if !handle.owner.belongs_to(&owner) {
                return Err(handle_access_error(&handle.owner, &owner));
            }
            fingerprint_handles.push(id.to_string());
            chat_attachments.push(handle.chat_attachment());
            agent_attachments.push(handle.agent_attachment()?);
        }

        let expires_at = self.expires_at();
        for handle_id in handles {
            let id = handle_id.as_str();
            state
                .handles
                .get_mut(id)
                .expect("validated attachment handle remains registered")
                .expires_at = expires_at;
            state.reserved_handles.insert(id.to_string());
        }
        let handle_ids = fingerprint_handles.clone();
        let attachments = ResolvedSendAttachments {
            chat_attachments,
            agent_attachments,
            #[cfg(test)]
            fingerprint_handles,
        };
        drop(state);

        Ok(AttachmentSendReservation::new(
            self.clone(),
            handle_ids,
            attachments,
        ))
    }

    pub(crate) fn resolve_for_reveal(
        &self,
        owner: impl Into<AttachmentOwner>,
        handle_id: &AttachmentHandleId,
    ) -> Result<ResolvedRevealAttachment, AttachmentRuntimeError> {
        let owner = owner.into();
        let mut state = self
            .state
            .lock()
            .expect("attachment runtime mutex poisoned");
        state.prune_expired(Instant::now());
        let handle = state
            .handles
            .get(handle_id.as_str())
            .ok_or(AttachmentRuntimeError::UnknownHandle)?;
        if !handle.owner.belongs_to(&owner) {
            return Err(handle_access_error(&handle.owner, &owner));
        }
        match &handle.target {
            AttachmentTarget::FileReference { path, allowed_root } => {
                allowed_root.validate_file(path)?;
                Ok(ResolvedRevealAttachment {
                    path: path.clone(),
                    label: handle.label.clone(),
                })
            }
            AttachmentTarget::EmbeddedSnapshot { .. } | AttachmentTarget::PastedImage { .. } => {
                Err(AttachmentRuntimeError::NotFile)
            }
        }
    }
}

fn handle_access_error(
    actual_owner: &AttachmentOwner,
    requested_owner: &AttachmentOwner,
) -> AttachmentRuntimeError {
    if actual_owner.belongs_to_task(requested_owner) {
        AttachmentRuntimeError::UnknownHandle
    } else {
        AttachmentRuntimeError::WrongTask
    }
}
