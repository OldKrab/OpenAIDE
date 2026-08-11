use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot;

use crate::snapshots::task_snapshot::TaskHistorySyncSnapshotSource;
use crate::tasks::task_operation::{PassiveTaskOperation, TaskOperationCoordinator};

#[cfg(test)]
#[path = "history_sync_tests.rs"]
mod tests;

/// Serializes Native Session history reconciliation for one Task.
#[derive(Clone, Default)]
pub(super) struct HistorySyncCoordinator {
    tasks: Arc<Mutex<HashMap<String, TaskSyncState>>>,
    operations: TaskOperationCoordinator,
}

#[derive(Default)]
struct TaskSyncState {
    generation: u64,
    in_flight_generation: Option<u64>,
    current: TaskHistorySyncSnapshot,
}

#[derive(Clone)]
pub(super) struct PassiveSyncGeneration {
    value: u64,
    operation: PassiveTaskOperation,
}

impl PassiveSyncGeneration {
    pub(super) fn value(&self) -> u64 {
        self.value
    }
}

impl HistorySyncCoordinator {
    pub(super) fn new(operations: TaskOperationCoordinator) -> Self {
        Self {
            tasks: Default::default(),
            operations,
        }
    }

    pub(super) fn clear_task_state(&self) {
        self.tasks
            .lock()
            .expect("history sync registry poisoned")
            .clear();
    }

    /// Registers one passive history-check generation.
    ///
    /// Catalog observations coalesce behind an existing check. A replacement owns
    /// the visible synchronization generation until it publishes a terminal state.
    pub(super) fn begin_passive(&self, task_id: &str) -> Option<PassiveSyncGeneration> {
        let operation = self.operations.begin_passive(task_id);
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let state = tasks.entry(task_id.to_string()).or_default();
        if state.in_flight_generation.is_some() {
            return None;
        }
        state.generation = state.generation.wrapping_add(1);
        state.in_flight_generation = Some(state.generation);
        Some(PassiveSyncGeneration {
            value: state.generation,
            operation,
        })
    }

    /// Runs the session-owning phase only if no user operation superseded discovery.
    pub(super) fn run_passive<T>(
        &self,
        task_id: &str,
        generation: &PassiveSyncGeneration,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        self.operations
            .try_serialize_passive(&generation.operation, || {
                let is_current = self
                    .tasks
                    .lock()
                    .expect("history sync registry poisoned")
                    .get(task_id)
                    .is_some_and(|state| {
                        state.generation == generation.value
                            && state.in_flight_generation == Some(generation.value)
                    });
                is_current.then(operation)
            })?
    }

    /// Releases a passive generation after its terminal snapshot was published.
    pub(super) fn finish_passive(&self, task_id: &str, generation: &PassiveSyncGeneration) {
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let Some(state) = tasks.get_mut(task_id) else {
            return;
        };
        if state.in_flight_generation == Some(generation.value) {
            state.in_flight_generation = None;
        }
    }

    pub(super) fn is_current(&self, task_id: &str, generation: &PassiveSyncGeneration) -> bool {
        self.tasks
            .lock()
            .expect("history sync registry poisoned")
            .get(task_id)
            .is_some_and(|state| state.generation == generation.value)
    }

    /// Catalog observation does not acquire the session gate. It can surface a durable
    /// possible-change hint while a direct resume finishes the current attachment.
    pub(super) fn reload_available_snapshot(&self, task_id: &str) -> TaskHistorySyncSnapshot {
        let generation = self
            .tasks
            .lock()
            .expect("history sync registry poisoned")
            .entry(task_id.to_string())
            .or_default()
            .generation;
        TaskHistorySyncSnapshot::ReloadAvailable { generation }
    }

    /// Starts a user-approved replay after its caller has acquired the Task operation gate.
    pub(super) fn begin_interactive(&self, task_id: &str) -> u64 {
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let state = tasks.entry(task_id.to_string()).or_default();
        state.generation = state.generation.wrapping_add(1);
        state.in_flight_generation = Some(state.generation);
        state.generation
    }

    pub(super) fn finish_interactive(&self, task_id: &str, generation: u64) {
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        if let Some(state) = tasks.get_mut(task_id) {
            if state.in_flight_generation == Some(generation) {
                state.in_flight_generation = None;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn has_in_flight_generation(&self, task_id: &str) -> bool {
        self.tasks
            .lock()
            .expect("history sync registry poisoned")
            .get(task_id)
            .is_some_and(|state| state.in_flight_generation.is_some())
    }

    /// Records only the state owned by the Task's current generation.
    pub(super) fn set_current(&self, task_id: &str, current: TaskHistorySyncSnapshot) -> bool {
        let generation = history_sync_generation(&current);
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let state = tasks.entry(task_id.to_string()).or_default();
        if generation != state.generation {
            return false;
        }
        state.current = current;
        true
    }
}

impl TaskHistorySyncSnapshotSource for HistorySyncCoordinator {
    fn history_sync_snapshot(&self, task_id: &str) -> TaskHistorySyncSnapshot {
        self.tasks
            .lock()
            .expect("history sync registry poisoned")
            .get(task_id)
            .map(|state| state.current.clone())
            .unwrap_or_default()
    }
}

fn history_sync_generation(snapshot: &TaskHistorySyncSnapshot) -> u64 {
    match snapshot {
        TaskHistorySyncSnapshot::Idle { generation }
        | TaskHistorySyncSnapshot::ReloadAvailable { generation }
        | TaskHistorySyncSnapshot::Syncing { generation }
        | TaskHistorySyncSnapshot::Updated { generation } => *generation,
    }
}
