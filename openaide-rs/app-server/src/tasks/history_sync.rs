use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use openaide_app_server_protocol::errors::ProtocolError;

use crate::protocol::model::AgentListedSession;
use crate::tasks::product_api::send::PendingSendSync;

const SESSION_LIST_FRESHNESS: Duration = Duration::from_secs(30);

/// Serializes Native Session reconciliation and send startup for one Task.
#[derive(Clone, Default)]
pub(super) struct HistorySyncCoordinator {
    tasks: Arc<Mutex<HashMap<String, TaskSyncState>>>,
    listings: NativeSessionCache,
    pending_sends: Arc<Mutex<HashMap<String, PendingSendSync>>>,
}

#[derive(Default)]
struct TaskSyncState {
    generation: u64,
    operation_lock: Arc<Mutex<()>>,
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

    /// Retains accepted send inputs when synchronization fails so Retry never duplicates Chat.
    pub(super) fn defer_send(&self, task_id: &str, pending: PendingSendSync) {
        self.pending_sends
            .lock()
            .expect("pending history sync registry poisoned")
            .insert(task_id.to_string(), pending);
    }

    pub(super) fn take_deferred_send(&self, task_id: &str) -> Option<PendingSendSync> {
        self.pending_sends
            .lock()
            .expect("pending history sync registry poisoned")
            .remove(task_id)
    }

    /// Registers advisory discovery work. A send can supersede it without waiting for session/list.
    pub(super) fn begin_passive(&self, task_id: &str) -> PassiveSyncGeneration {
        let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
        let state = tasks.entry(task_id.to_string()).or_default();
        state.generation = state.generation.wrapping_add(1);
        PassiveSyncGeneration(state.generation)
    }

    /// Runs the session-owning phase only if no send superseded the discovery generation.
    pub(super) fn run_passive<T>(
        &self,
        task_id: &str,
        generation: PassiveSyncGeneration,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        let operation_lock = {
            let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
            let state = tasks.entry(task_id.to_string()).or_default();
            if state.generation != generation.0 {
                return None;
            }
            state.operation_lock.clone()
        };
        let _guard = operation_lock
            .lock()
            .expect("task history sync lock poisoned");
        let is_current = self
            .tasks
            .lock()
            .expect("history sync registry poisoned")
            .get(task_id)
            .is_some_and(|state| state.generation == generation.0);
        is_current.then(operation)
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
        state.generation
    }

    pub(super) fn is_generation_current(&self, task_id: &str, generation: u64) -> bool {
        self.tasks
            .lock()
            .expect("history sync registry poisoned")
            .get(task_id)
            .is_some_and(|state| state.generation == generation)
    }

    /// Invalidates passive discovery before waiting for any session-owning work already underway.
    pub(super) fn run_send<T>(
        &self,
        task_id: &str,
        generation: u64,
        operation: impl FnOnce() -> T,
    ) -> T {
        let operation_lock = {
            let mut tasks = self.tasks.lock().expect("history sync registry poisoned");
            let state = tasks.entry(task_id.to_string()).or_default();
            state.generation = state.generation.max(generation);
            state.operation_lock.clone()
        };
        let _guard = operation_lock
            .lock()
            .expect("task history sync lock poisoned");
        operation()
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
