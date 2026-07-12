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

/// Serializes Native Session history reconciliation for one Task.
#[derive(Clone, Default)]
pub(super) struct HistorySyncCoordinator {
    tasks: Arc<Mutex<HashMap<String, TaskSyncState>>>,
    listings: NativeSessionCache,
    operations: TaskOperationCoordinator,
}

#[derive(Default)]
struct TaskSyncState {
    generation: u64,
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

    /// Registers one passive history-check generation.
    pub(super) fn begin_passive(&self, task_id: &str) -> Option<PassiveSyncGeneration> {
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let state = tasks.entry(task_id.to_string()).or_default();
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
