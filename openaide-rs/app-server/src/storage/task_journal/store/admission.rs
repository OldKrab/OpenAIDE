use std::sync::mpsc;

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;
use crate::storage::task_journal::model::{CommittedTaskBatch, TaskWrite};

use super::TaskJournalStore;

/// Handle for one admitted write. Waiting establishes durability; dropping the
/// handle leaves the admitted write owned by the storage worker.
pub struct CommitReceipt {
    receiver: mpsc::Receiver<Result<CommittedTaskBatch, RuntimeError>>,
}

impl CommitReceipt {
    pub(super) fn new(receiver: mpsc::Receiver<Result<CommittedTaskBatch, RuntimeError>>) -> Self {
        Self { receiver }
    }

    pub fn wait(self) -> Result<CommittedTaskBatch, RuntimeError> {
        self.receiver.recv().map_err(|_| {
            RuntimeError::Storage("Task journal worker stopped before commit".to_string())
        })?
    }
}

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
            None => Ok(TrySubmit::Admitted(CommitReceipt::new(receiver))),
        }
    }

    pub(crate) fn wait_for_capacity(&self, write: &TaskWrite) -> Result<(), RuntimeError> {
        self.inner.scheduler.wait_for_capacity(write)
    }
}
