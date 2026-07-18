pub mod app_preferences;
pub mod atomic;
pub mod cursor;
pub mod id;
pub mod message_store;
pub mod new_task_defaults;
pub mod records;
pub mod root;
pub mod task_store;
pub mod tool_artifacts;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

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
    settings_write_lock: Mutex<()>,
    worktree_write_lock: Mutex<()>,
    agent_message_cache: Mutex<HashMap<String, message_store::AgentMessageCache>>,
    #[cfg(test)]
    fail_next_task_write: AtomicBool,
    #[cfg(test)]
    message_file_write_count: AtomicUsize,
    #[cfg(test)]
    message_file_read_count: AtomicUsize,
    #[cfg(test)]
    after_next_task_snapshot_read: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    after_next_task_read: Mutex<Option<Box<dyn FnOnce() + Send>>>,
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
        std::fs::create_dir_all(root.join("worktrees"))?;

        Ok(Self {
            inner: Arc::new(StoreInner {
                root,
                recovery: open.recovery,
                open_guard: open.guard,
                settings_write_lock: Mutex::new(()),
                worktree_write_lock: Mutex::new(()),
                agent_message_cache: Mutex::new(HashMap::new()),
                #[cfg(test)]
                fail_next_task_write: AtomicBool::new(false),
                #[cfg(test)]
                message_file_write_count: AtomicUsize::new(0),
                #[cfg(test)]
                message_file_read_count: AtomicUsize::new(0),
                #[cfg(test)]
                after_next_task_snapshot_read: Mutex::new(None),
                #[cfg(test)]
                after_next_task_read: Mutex::new(None),
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

    pub fn worktrees_dir(&self) -> PathBuf {
        self.inner.root.join("worktrees")
    }

    pub(crate) fn lock_settings_write(&self) -> std::sync::MutexGuard<'_, ()> {
        self.inner
            .settings_write_lock
            .lock()
            .expect("settings write lock poisoned")
    }

    pub(crate) fn lock_worktree_write(&self) -> std::sync::MutexGuard<'_, ()> {
        self.inner
            .worktree_write_lock
            .lock()
            .expect("worktree catalog lock poisoned")
    }

    pub fn task_dir(&self, task_id: &str) -> Result<PathBuf, RuntimeError> {
        id::validate_task_id(task_id)?;
        Ok(self.tasks_dir().join(task_id))
    }

    #[cfg(test)]
    pub(crate) fn fail_next_task_write_for_test(&self) {
        self.inner
            .fail_next_task_write
            .store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn take_task_write_failure_for_test(&self) -> bool {
        self.inner
            .fail_next_task_write
            .swap(false, Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn message_file_write_count_for_test(&self) -> usize {
        self.inner.message_file_write_count.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn message_file_read_count_for_test(&self) -> usize {
        self.inner.message_file_read_count.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn after_next_task_snapshot_read_for_test(
        &self,
        hook: impl FnOnce() + Send + 'static,
    ) {
        *self
            .inner
            .after_next_task_snapshot_read
            .lock()
            .expect("Task snapshot hook poisoned") = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(crate) fn run_after_task_snapshot_read_hook_for_test(&self) {
        let hook = self
            .inner
            .after_next_task_snapshot_read
            .lock()
            .expect("Task snapshot hook poisoned")
            .take();
        if let Some(hook) = hook {
            hook();
        }
    }

    #[cfg(test)]
    pub(crate) fn after_next_task_read_for_test(&self, hook: impl FnOnce() + Send + 'static) {
        *self
            .inner
            .after_next_task_read
            .lock()
            .expect("Task read hook poisoned") = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(super) fn run_after_task_read_hook_for_test(&self) {
        let hook = self
            .inner
            .after_next_task_read
            .lock()
            .expect("Task read hook poisoned")
            .take();
        if let Some(hook) = hook {
            hook();
        }
    }
}

#[cfg(test)]
mod tests;
