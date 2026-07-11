pub mod app_preferences;
pub mod atomic;
pub mod cursor;
pub mod id;
pub mod message_store;
pub mod records;
pub mod root;
pub mod send_receipts;
pub mod task_store;
pub mod tool_artifacts;

use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::protocol::errors::RuntimeError;
use crate::storage_runtime::{RecoveryClassification, StorageOpenGuard};

pub use crate::storage_runtime::StorageOpenError as StoreOpenError;

#[derive(Clone)]
pub struct Store {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    root: PathBuf,
    recovery: RecoveryClassification,
    open_guard: StorageOpenGuard,
    #[cfg(test)]
    fail_next_tail_page: AtomicBool,
    #[cfg(test)]
    crash_before_next_task_write: AtomicBool,
    #[cfg(test)]
    message_file_write_count: AtomicUsize,
}

impl From<StoreOpenError> for RuntimeError {
    fn from(value: StoreOpenError) -> Self {
        RuntimeError::Storage(value.to_string())
    }
}

impl Store {
    pub fn open(root: PathBuf) -> Result<Self, StoreOpenError> {
        let open = StorageOpenGuard::open(&root)?;
        std::fs::create_dir_all(root.join("tasks"))?;
        std::fs::create_dir_all(root.join("diagnostics"))?;
        std::fs::create_dir_all(root.join("agents"))?;
        std::fs::create_dir_all(root.join("settings"))?;

        Ok(Self {
            inner: Arc::new(StoreInner {
                root,
                recovery: open.recovery,
                open_guard: open.guard,
                #[cfg(test)]
                fail_next_tail_page: AtomicBool::new(false),
                #[cfg(test)]
                crash_before_next_task_write: AtomicBool::new(false),
                #[cfg(test)]
                message_file_write_count: AtomicUsize::new(0),
            }),
        })
    }

    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    pub fn recovery_classification(&self) -> RecoveryClassification {
        self.inner.recovery
    }

    pub fn mark_clean_shutdown(&self) -> Result<(), RuntimeError> {
        self.inner.open_guard.mark_clean_shutdown()?;
        Ok(())
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.inner.root.join("tasks")
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.inner.root.join("agents")
    }

    pub fn settings_dir(&self) -> PathBuf {
        self.inner.root.join("settings")
    }

    pub fn task_dir(&self, task_id: &str) -> Result<PathBuf, RuntimeError> {
        id::validate_task_id(task_id)?;
        Ok(self.tasks_dir().join(task_id))
    }

    #[cfg(test)]
    pub(crate) fn fail_next_tail_page_for_test(&self) {
        self.inner.fail_next_tail_page.store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn take_tail_page_failure_for_test(&self) -> bool {
        self.inner.fail_next_tail_page.swap(false, Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn crash_before_next_task_write_for_test(&self) {
        self.inner
            .crash_before_next_task_write
            .store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn take_task_write_crash_for_test(&self) -> bool {
        self.inner
            .crash_before_next_task_write
            .swap(false, Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn message_file_write_count_for_test(&self) -> usize {
        self.inner.message_file_write_count.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests;
