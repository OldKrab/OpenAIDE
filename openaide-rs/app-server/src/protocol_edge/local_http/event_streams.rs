use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::client_lifecycle::ConnectionId;

#[cfg(test)]
#[path = "event_streams_tests.rs"]
mod tests;

#[derive(Clone, Default)]
pub(super) struct EventStreamRegistry {
    state: Arc<Mutex<EventStreamState>>,
}

#[derive(Default)]
struct EventStreamState {
    next_generation: u64,
    active: HashMap<ConnectionId, u64>,
}

#[derive(Clone)]
pub(crate) struct EventStreamLease {
    connection_id: ConnectionId,
    generation: u64,
}

impl EventStreamRegistry {
    pub(super) fn begin(&self, connection_id: ConnectionId) -> EventStreamLease {
        let mut state = self
            .state
            .lock()
            .expect("event stream registry lock poisoned");
        state.next_generation = state.next_generation.saturating_add(1);
        let generation = state.next_generation;
        state.active.insert(connection_id.clone(), generation);
        EventStreamLease {
            connection_id,
            generation,
        }
    }

    pub(super) fn is_active(&self, connection_id: &ConnectionId) -> bool {
        self.state
            .lock()
            .expect("event stream registry lock poisoned")
            .active
            .contains_key(connection_id)
    }

    pub(super) fn is_current(&self, lease: &EventStreamLease) -> bool {
        self.state
            .lock()
            .expect("event stream registry lock poisoned")
            .active
            .get(&lease.connection_id)
            == Some(&lease.generation)
    }

    /// Keeps lease validation atomic with work that consumes stream-owned state.
    pub(super) fn with_current<R>(
        &self,
        lease: &EventStreamLease,
        action: impl FnOnce() -> R,
    ) -> Option<R> {
        let state = self
            .state
            .lock()
            .expect("event stream registry lock poisoned");
        (state.active.get(&lease.connection_id) == Some(&lease.generation)).then(action)
    }

    pub(super) fn finish(&self, lease: &EventStreamLease) {
        let mut state = self
            .state
            .lock()
            .expect("event stream registry lock poisoned");
        if state.active.get(&lease.connection_id) == Some(&lease.generation) {
            state.active.remove(&lease.connection_id);
        }
    }
}

impl EventStreamLease {
    pub(super) fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }
}
