use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::tasks::task_operation::TaskOperationCoordinator;

/// Owns durably accepted Turns until startup hands them to `TurnRunner` or retires them.
///
/// Persistence makes a Task established, but startup may still be waiting on Task preparation,
/// Native Session opening, or event attachment. This registry closes that lifecycle gap so an
/// accepted Turn is never mistaken for abandoned work and remains an idle-shutdown blocker.
#[derive(Clone, Default)]
pub(super) struct TurnAcceptanceCoordinator {
    acceptance: TaskOperationCoordinator,
    pending_turns: Arc<Mutex<HashMap<String, String>>>,
}

impl TurnAcceptanceCoordinator {
    /// Serializes Send acceptance for one Task through its durable commit.
    ///
    /// This protects the Task revision and attachment reservation as one admission decision;
    /// it does not retry requests or deduplicate them through a second request identity.
    pub(super) fn serialize<T>(&self, task_id: &str, operation: impl FnOnce() -> T) -> T {
        self.acceptance.serialize(task_id, operation)
    }

    pub(super) fn own_pending_turn(&self, task_id: &str, turn_id: &str) -> bool {
        let mut pending = self
            .pending_turns
            .lock()
            .expect("pending Turn ownership registry poisoned");
        match pending.get(task_id) {
            Some(owned_turn_id) => owned_turn_id == turn_id,
            None => {
                pending.insert(task_id.to_string(), turn_id.to_string());
                true
            }
        }
    }

    pub(super) fn owns_pending_turn(&self, task_id: &str, turn_id: &str) -> bool {
        self.pending_turns
            .lock()
            .expect("pending Turn ownership registry poisoned")
            .get(task_id)
            .is_some_and(|owned_turn_id| owned_turn_id == turn_id)
    }

    pub(super) fn retire_pending_turn(&self, task_id: &str, turn_id: &str) {
        let mut pending = self
            .pending_turns
            .lock()
            .expect("pending Turn ownership registry poisoned");
        if pending
            .get(task_id)
            .is_some_and(|owned_turn_id| owned_turn_id == turn_id)
        {
            pending.remove(task_id);
        }
    }

    pub(super) fn owned_turns(&self) -> HashSet<(String, String)> {
        self.pending_turns
            .lock()
            .expect("pending Turn ownership registry poisoned")
            .iter()
            .map(|(task_id, turn_id)| (task_id.clone(), turn_id.clone()))
            .collect()
    }
}
