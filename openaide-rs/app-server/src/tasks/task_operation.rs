use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError, Weak};

/// Serializes one operation family per Task without blocking unrelated Tasks.
///
/// Callers own the domain lifecycle; this primitive owns only lock identity and
/// removes idle lock entries after every queued caller has finished.
#[derive(Clone, Default)]
pub(super) struct TaskOperationCoordinator {
    locks: Arc<Mutex<HashMap<String, Weak<TaskOperationState>>>>,
}

struct TaskOperationState {
    lock: Mutex<()>,
    interaction_generation: AtomicU64,
}

#[derive(Clone)]
pub(super) struct PassiveTaskOperation {
    state: Arc<TaskOperationState>,
    interaction_generation: u64,
}

impl TaskOperationCoordinator {
    pub(super) fn serialize<T>(&self, task_id: &str, operation: impl FnOnce() -> T) -> T {
        let lock_use = self.lock_use(task_id);
        let _guard = lock_use
            .state
            .lock
            .lock()
            .expect("Task operation lock poisoned");
        lock_use
            .state
            .interaction_generation
            .fetch_add(1, Ordering::SeqCst);
        operation()
    }

    /// Captures the ordering point at which passive work was requested.
    pub(super) fn begin_passive(&self, task_id: &str) -> PassiveTaskOperation {
        let lock_use = self.lock_use(task_id);
        PassiveTaskOperation {
            interaction_generation: lock_use.state.interaction_generation.load(Ordering::SeqCst),
            state: lock_use.state.clone(),
        }
    }

    /// Runs passive work only if no user operation won the Task gate afterward.
    pub(super) fn try_serialize_passive<T>(
        &self,
        passive: &PassiveTaskOperation,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        let _guard = match passive.state.lock.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(_)) => panic!("Task operation lock poisoned"),
        };
        if passive.state.interaction_generation.load(Ordering::SeqCst)
            != passive.interaction_generation
        {
            return None;
        }
        Some(operation())
    }

    fn lock_use(&self, task_id: &str) -> TaskOperationLockUse {
        let operation_lock = {
            let mut locks = self
                .locks
                .lock()
                .expect("Task operation lock registry poisoned");
            match locks.get(task_id).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(TaskOperationState {
                        lock: Mutex::new(()),
                        interaction_generation: AtomicU64::new(0),
                    });
                    locks.insert(task_id.to_string(), Arc::downgrade(&lock));
                    lock
                }
            }
        };
        TaskOperationLockUse {
            registry: self.locks.clone(),
            task_id: task_id.to_string(),
            state: operation_lock,
        }
    }
}

/// Queued callers hold strong references, so cleanup cannot split them across
/// different mutexes even when the current owner is unwinding.
struct TaskOperationLockUse {
    registry: Arc<Mutex<HashMap<String, Weak<TaskOperationState>>>>,
    task_id: String,
    state: Arc<TaskOperationState>,
}

impl Drop for TaskOperationLockUse {
    fn drop(&mut self) {
        let mut locks = self
            .registry
            .lock()
            .expect("Task operation lock registry poisoned");
        if Arc::strong_count(&self.state) == 1
            && locks
                .get(&self.task_id)
                .is_some_and(|registered| registered.ptr_eq(&Arc::downgrade(&self.state)))
        {
            locks.remove(&self.task_id);
        }
    }
}
