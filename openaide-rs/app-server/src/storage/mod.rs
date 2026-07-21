pub mod app_preferences;
pub mod atomic;
pub mod cursor;
pub mod id;
pub mod message_store;
pub mod new_task_defaults;
pub mod records;
pub mod root;
pub mod task_journal;
pub mod task_store;
pub mod tool_artifacts;

use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::atomic::{AtomicBool as RuntimeAtomicBool, Ordering as RuntimeOrdering};
use std::sync::Arc;
use std::sync::{Mutex, RwLock};
use std::time::Duration;

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
    /// Sole durable owner for Task, Chat, and Tool-detail state.
    task_journal: task_journal::TaskJournalStore,
    task_commit_handler: Arc<RwLock<Option<TaskCommitHandler>>>,
    task_commit_dispatch_stop: Arc<RuntimeAtomicBool>,
    task_commit_dispatch_worker: Mutex<Option<std::thread::JoinHandle<()>>>,
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

type TaskCommitHandler = Arc<dyn Fn(task_journal::CommittedTaskBatch) + Send + Sync + 'static>;

impl Drop for StoreInner {
    fn drop(&mut self) {
        self.task_commit_dispatch_stop
            .store(true, RuntimeOrdering::Release);
        if let Some(worker) = self
            .task_commit_dispatch_worker
            .get_mut()
            .expect("Task commit dispatch worker poisoned")
            .take()
        {
            let _ = worker.join();
        }
    }
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
        let (task_journal, initial_commit_events) =
            task_journal::TaskJournalStore::open(root.clone())
                .map_err(|error| StoreOpenError::TaskStorage(error.to_string()))?;
        let task_commit_handler = Arc::new(RwLock::new(None::<TaskCommitHandler>));
        let task_commit_dispatch_stop = Arc::new(RuntimeAtomicBool::new(false));
        let worker_handler = task_commit_handler.clone();
        let worker_stop = task_commit_dispatch_stop.clone();
        let task_commit_dispatch_worker = std::thread::Builder::new()
            .name("openaide-task-commit-dispatch".to_string())
            .spawn(move || {
                crate::logging::info("task_commit_dispatch_started", serde_json::json!({}));
                while !worker_stop.load(RuntimeOrdering::Acquire) {
                    let committed =
                        match initial_commit_events.recv_timeout(Duration::from_millis(25)) {
                            Ok(committed) => committed,
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        };
                    let handler = worker_handler
                        .read()
                        .expect("Task commit handler poisoned")
                        .clone();
                    if let Some(handler) = handler {
                        handler(committed);
                    }
                }
                crate::logging::info("task_commit_dispatch_stopped", serde_json::json!({}));
            })
            .map_err(StoreOpenError::Io)?;

        Ok(Self {
            inner: Arc::new(StoreInner {
                root,
                recovery: open.recovery,
                open_guard: open.guard,
                settings_write_lock: Mutex::new(()),
                worktree_write_lock: Mutex::new(()),
                task_journal,
                task_commit_handler,
                task_commit_dispatch_stop,
                task_commit_dispatch_worker: Mutex::new(Some(task_commit_dispatch_worker)),
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
        // A clean process marker may only follow a fully drained Task journal.
        self.inner.task_journal.shutdown()?;
        self.inner.open_guard.mark_clean_shutdown()?;
        Ok(())
    }

    pub(crate) fn task_journal(&self) -> &task_journal::TaskJournalStore {
        &self.inner.task_journal
    }

    /// Installs the current Task runtime's terminal-detail publication sink.
    /// Store retains the sole commit receiver, so runtime replacement swaps a
    /// callback instead of creating competing storage subscriptions.
    pub(crate) fn set_task_commit_handler(&self, handler: TaskCommitHandler) {
        *self
            .inner
            .task_commit_handler
            .write()
            .expect("Task commit handler poisoned") = Some(handler);
    }

    /// Transfers the sole root-wide storage failure stream to the process
    /// supervisor that owns App Server termination.
    pub(crate) fn take_task_storage_fatal_events(
        &self,
    ) -> std::sync::mpsc::Receiver<task_journal::TaskStorageFatalFailure> {
        self.inner.task_journal.take_fatal_events()
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
    pub(crate) fn take_task_write_failure_for_test(&self) -> bool {
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
