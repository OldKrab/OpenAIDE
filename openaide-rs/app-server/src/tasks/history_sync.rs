use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot;

use crate::protocol::model::AgentListedSession;
use crate::snapshots::task_snapshot::TaskHistorySyncSnapshotSource;
use crate::tasks::task_operation::TaskOperationCoordinator;

#[cfg(test)]
#[path = "history_sync_tests.rs"]
mod tests;

const SESSION_LIST_FRESHNESS: Duration = Duration::from_secs(30);

/// Serializes Native Session reconciliation and send startup for one Task.
#[derive(Clone, Default)]
pub(super) struct HistorySyncCoordinator {
    tasks: Arc<Mutex<HashMap<String, TaskSyncState>>>,
    task_state_changed: Arc<Condvar>,
    listings: NativeSessionCache,
    operations: TaskOperationCoordinator,
}

#[derive(Default)]
struct TaskSyncState {
    generation: u64,
    current_send_generation: Option<u64>,
    current: TaskHistorySyncSnapshot,
}

#[derive(Clone, Copy)]
pub(super) struct PassiveSyncGeneration(u64);

impl PassiveSyncGeneration {
    pub(super) fn value(self) -> u64 {
        self.0
    }
}

impl HistorySyncCoordinator {
    /// Shares only complete successful pagination results. Failures wake waiters but are never cached.
    pub(super) fn listed_sessions(
        &self,
        agent_id: &str,
        workspace_root: &str,
        fetch: impl FnOnce() -> Result<Vec<AgentListedSession>, ProtocolError>,
    ) -> Result<Vec<AgentListedSession>, ProtocolError> {
        self.listings.get_or_fetch(agent_id, workspace_root, fetch)
    }

    /// Registers advisory discovery work. A send can supersede it without waiting for session/list.
    pub(super) fn begin_passive(&self, task_id: &str) -> Option<PassiveSyncGeneration> {
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let state = tasks.entry(task_id.to_string()).or_default();
        if state.current_send_generation.is_some() {
            return None;
        }
        state.generation = state.generation.wrapping_add(1);
        Some(PassiveSyncGeneration(state.generation))
    }

    /// Runs the session-owning phase only if no send superseded the discovery generation.
    pub(super) fn run_passive<T>(
        &self,
        task_id: &str,
        generation: PassiveSyncGeneration,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        self.operations.serialize(task_id, || {
            let is_current = self
                .tasks
                .lock()
                .expect("history sync registry poisoned")
                .get(task_id)
                .is_some_and(|state| state.generation == generation.0);
            is_current.then(operation)
        })
    }

    pub(super) fn is_current(&self, task_id: &str, generation: PassiveSyncGeneration) -> bool {
        self.tasks
            .lock()
            .expect("history sync registry poisoned")
            .get(task_id)
            .is_some_and(|state| state.generation == generation.0)
    }

    pub(super) fn begin_send(&self, task_id: &str) -> u64 {
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let state = tasks.entry(task_id.to_string()).or_default();
        state.generation = state.generation.wrapping_add(1);
        state.current_send_generation = Some(state.generation);
        let generation = state.generation;
        self.task_state_changed.notify_all();
        generation
    }

    /// Records only the state owned by the Task's current generation.
    pub(super) fn set_current(&self, task_id: &str, current: TaskHistorySyncSnapshot) -> bool {
        let generation = history_sync_generation(&current);
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let state = tasks.entry(task_id.to_string()).or_default();
        if generation != state.generation {
            return false;
        }
        let send_still_owns_history_sync = matches!(
            &current,
            TaskHistorySyncSnapshot::Syncing { .. }
                | TaskHistorySyncSnapshot::Failed {
                    before_send: true,
                    ..
                }
        );
        if state.current_send_generation == Some(generation) && !send_still_owns_history_sync {
            state.current_send_generation = None;
        }
        state.current = current;
        true
    }

    pub(super) fn is_generation_current(&self, task_id: &str, generation: u64) -> bool {
        self.tasks
            .lock()
            .expect("history sync registry poisoned")
            .get(task_id)
            .is_some_and(|state| state.current_send_generation == Some(generation))
    }

    /// Runs only the send that still owns the Task's current send generation.
    pub(super) fn run_send<T>(
        &self,
        task_id: &str,
        generation: u64,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        self.operations.serialize(task_id, || {
            self.is_generation_current(task_id, generation)
                .then(operation)
        })
    }

    /// Waits without polling while a send still owns its generation. State producers must
    /// call `notify_task_state_changed` after persisting a value that can finish `inspect`.
    pub(super) fn wait_for_current_send<T, E>(
        &self,
        task_id: &str,
        generation: u64,
        mut inspect: impl FnMut() -> Result<Option<T>, E>,
    ) -> Result<Option<T>, E> {
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        loop {
            let is_current = tasks
                .get(task_id)
                .is_some_and(|state| state.current_send_generation == Some(generation));
            if !is_current {
                return Ok(None);
            }
            if let Some(value) = inspect()? {
                return Ok(Some(value));
            }
            tasks = self
                .task_state_changed
                .wait(tasks)
                .expect("history sync state wait poisoned");
        }
    }

    /// Completes the no-missed-wakeup handshake for persisted Task state observed by send waits.
    pub(super) fn notify_task_state_changed(&self) {
        let tasks = self.tasks.lock().expect("history sync registry poisoned");
        self.task_state_changed.notify_all();
        drop(tasks);
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
        | TaskHistorySyncSnapshot::Checking { generation }
        | TaskHistorySyncSnapshot::Syncing { generation }
        | TaskHistorySyncSnapshot::Updated { generation }
        | TaskHistorySyncSnapshot::Failed { generation, .. } => *generation,
    }
}

#[derive(Clone, Default)]
struct NativeSessionCache {
    shared: Arc<(Mutex<CacheState>, Condvar)>,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<CacheKey, CacheEntry>,
    in_flight: HashSet<CacheKey>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct CacheKey {
    agent_id: String,
    workspace_root: String,
}

struct CacheEntry {
    fetched_at: Instant,
    sessions: Vec<AgentListedSession>,
}

impl NativeSessionCache {
    fn get_or_fetch(
        &self,
        agent_id: &str,
        workspace_root: &str,
        fetch: impl FnOnce() -> Result<Vec<AgentListedSession>, ProtocolError>,
    ) -> Result<Vec<AgentListedSession>, ProtocolError> {
        let key = CacheKey {
            agent_id: agent_id.to_string(),
            workspace_root: workspace_root.to_string(),
        };
        let (state_lock, ready) = &*self.shared;
        let mut state = state_lock.lock().expect("native session cache poisoned");
        loop {
            if let Some(entry) = state.entries.get(&key) {
                if entry.fetched_at.elapsed() < SESSION_LIST_FRESHNESS {
                    return Ok(entry.sessions.clone());
                }
            }
            if state.in_flight.insert(key.clone()) {
                break;
            }
            state = ready
                .wait(state)
                .expect("native session cache wait poisoned");
        }
        drop(state);

        let result = fetch();
        let mut state = state_lock.lock().expect("native session cache poisoned");
        state.in_flight.remove(&key);
        if let Ok(sessions) = &result {
            state.entries.insert(
                key,
                CacheEntry {
                    fetched_at: Instant::now(),
                    sessions: sessions.clone(),
                },
            );
        }
        ready.notify_all();
        result
    }
}
