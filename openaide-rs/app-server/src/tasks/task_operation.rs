use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

/// Serializes one operation family per Task without blocking unrelated Tasks.
///
/// Callers own the domain lifecycle; this primitive owns only lock identity and
/// removes idle lock entries after every queued caller has finished.
#[derive(Clone, Default)]
pub(super) struct TaskOperationCoordinator {
    locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
}

impl TaskOperationCoordinator {
    pub(super) fn serialize<T>(&self, task_id: &str, operation: impl FnOnce() -> T) -> T {
        let operation_lock = {
            let mut locks = self
                .locks
                .lock()
                .expect("Task operation lock registry poisoned");
            match locks.get(task_id).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(Mutex::new(()));
                    locks.insert(task_id.to_string(), Arc::downgrade(&lock));
                    lock
                }
            }
        };
        let lock_use = TaskOperationLockUse {
            registry: self.locks.clone(),
            task_id: task_id.to_string(),
            lock: operation_lock,
        };
        let _guard = lock_use.lock.lock().expect("Task operation lock poisoned");
        operation()
    }
}

/// Queued callers hold strong references, so cleanup cannot split them across
/// different mutexes even when the current owner is unwinding.
struct TaskOperationLockUse {
    registry: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
    task_id: String,
    lock: Arc<Mutex<()>>,
}

impl Drop for TaskOperationLockUse {
    fn drop(&mut self) {
        let mut locks = self
            .registry
            .lock()
            .expect("Task operation lock registry poisoned");
        if Arc::strong_count(&self.lock) == 1
            && locks
                .get(&self.task_id)
                .is_some_and(|registered| registered.ptr_eq(&Arc::downgrade(&self.lock)))
        {
            locks.remove(&self.task_id);
        }
    }
}
