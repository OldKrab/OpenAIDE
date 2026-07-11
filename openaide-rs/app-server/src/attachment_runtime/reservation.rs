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

    pub(crate) fn attachments(&self) -> &ResolvedSendAttachments {
        self.attachments
            .as_ref()
            .expect("attachment reservation has not been committed")
    }

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
