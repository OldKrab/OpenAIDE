use std::sync::mpsc;

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;
use crate::storage::task_journal::model::TaskWrite;

use super::{CommitReceipt, TaskJournalStore};

pub(crate) enum TrySubmit {
    Admitted(CommitReceipt),
    Full(TaskWrite),
}

impl TaskJournalStore {
    pub(crate) fn try_submit(&self, write: TaskWrite) -> Result<TrySubmit, RuntimeError> {
        validate_task_id(&write.task_id)?;
        let (reply, receiver) = mpsc::channel();
        match self.inner.scheduler.try_admit(write, reply)? {
            Some(write) => Ok(TrySubmit::Full(write)),
            None => Ok(TrySubmit::Admitted(CommitReceipt { receiver })),
        }
    }

    pub(crate) fn wait_for_capacity(&self, write: &TaskWrite) -> Result<(), RuntimeError> {
        self.inner.scheduler.wait_for_capacity(write)
    }
}
