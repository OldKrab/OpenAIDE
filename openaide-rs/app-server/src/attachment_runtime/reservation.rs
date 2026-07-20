use super::{AttachmentRuntime, ResolvedSendAttachments};

pub(crate) struct AttachmentSendReservation {
    runtime: AttachmentRuntime,
    handle_ids: Vec<String>,
    attachments: Option<ResolvedSendAttachments>,
    committed: bool,
}

impl AttachmentSendReservation {
    pub(super) fn new(
        runtime: AttachmentRuntime,
        handle_ids: Vec<String>,
        attachments: ResolvedSendAttachments,
    ) -> Self {
        Self {
            runtime,
            handle_ids,
            attachments: Some(attachments),
            committed: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn commit(mut self) -> ResolvedSendAttachments {
        {
            let mut state = self
                .runtime
                .state
                .lock()
                .expect("attachment runtime mutex poisoned");
            for handle_id in &self.handle_ids {
                state.handles.remove(handle_id);
                state.reserved_handles.remove(handle_id);
            }
        }
        self.committed = true;
        self.attachments
            .take()
            .expect("attachment reservation has resolved attachments")
    }

    pub(crate) fn resolved_with_inline_images(
        &self,
        images: &[openaide_app_server_protocol::task::ComposerImage],
    ) -> Result<ResolvedSendAttachments, super::AttachmentRuntimeError> {
        self.attachments
            .as_ref()
            .expect("attachment reservation has resolved attachments")
            .clone_with_inline_images(images)
    }

    pub(crate) fn commit_with(
        mut self,
        attachments: ResolvedSendAttachments,
    ) -> ResolvedSendAttachments {
        {
            let mut state = self
                .runtime
                .state
                .lock()
                .expect("attachment runtime mutex poisoned");
            for handle_id in &self.handle_ids {
                state.handles.remove(handle_id);
                state.reserved_handles.remove(handle_id);
            }
        }
        self.committed = true;
        self.attachments = None;
        attachments
    }
}

impl Drop for AttachmentSendReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let Ok(mut state) = self.runtime.state.lock() else {
            return;
        };
        for handle_id in &self.handle_ids {
            state.reserved_handles.remove(handle_id);
        }
    }
}
