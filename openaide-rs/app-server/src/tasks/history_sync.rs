use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot;

use crate::protocol::model::AgentListedSession;
use crate::snapshots::task_snapshot::TaskHistorySyncSnapshotSource;
use crate::tasks::task_operation::TaskOperationCoordinator;

#[cfg(test)]
#[path = "history_sync_tests.rs"]
mod tests;

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
    /// Returns only catalog state already refreshed outside Task opening.
    pub(super) fn cached_session(
        &self,
        agent_id: &str,
        workspace_root: &str,
        session_id: &str,
    ) -> Option<AgentListedSession> {
        self.listings
            .get(agent_id, workspace_root)
            .into_iter()
            .find(|session| session.session_id == session_id)
    }

    /// Merges one successful Native Session page into the shared catalog.
    pub(super) fn record_listed_sessions(
        &self,
        agent_id: &str,
        workspace_root: &str,
        sessions: &[AgentListedSession],
    ) {
        self.listings.merge(agent_id, workspace_root, sessions);
    }

    /// Replaces one fully paged catalog only after the refresh succeeds.
    pub(super) fn replace_listed_sessions(
        &self,
        agent_id: &str,
        workspace_root: &str,
        sessions: Vec<AgentListedSession>,
    ) {
        self.listings.replace(agent_id, workspace_root, sessions);
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
        | TaskHistorySyncSnapshot::Syncing { generation }
        | TaskHistorySyncSnapshot::Updated { generation } => *generation,
    }
}

#[derive(Clone, Default)]
struct NativeSessionCache {
    entries: Arc<Mutex<HashMap<CacheKey, Vec<AgentListedSession>>>>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct CacheKey {
    agent_id: String,
    workspace_root: String,
}

impl NativeSessionCache {
    fn get(&self, agent_id: &str, workspace_root: &str) -> Vec<AgentListedSession> {
        let key = CacheKey {
            agent_id: agent_id.to_string(),
            workspace_root: workspace_root.to_string(),
        };
        self.entries
            .lock()
            .expect("native session cache poisoned")
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    fn merge(&self, agent_id: &str, workspace_root: &str, sessions: &[AgentListedSession]) {
        let key = CacheKey {
            agent_id: agent_id.to_string(),
            workspace_root: workspace_root.to_string(),
        };
        let mut entries = self.entries.lock().expect("native session cache poisoned");
        let entry = entries.entry(key).or_default();
        for session in sessions {
            if let Some(existing) = entry
                .iter_mut()
                .find(|existing| existing.session_id == session.session_id)
            {
                *existing = session.clone();
            } else {
                entry.push(session.clone());
            }
        }
    }

    fn replace(&self, agent_id: &str, workspace_root: &str, sessions: Vec<AgentListedSession>) {
        let key = CacheKey {
            agent_id: agent_id.to_string(),
            workspace_root: workspace_root.to_string(),
        };
        self.entries
            .lock()
            .expect("native session cache poisoned")
            .insert(key, sessions);
    }
}
