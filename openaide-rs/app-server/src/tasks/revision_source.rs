use std::sync::{Arc, Mutex};

use crate::tasks::runtime_state::RuntimeState;

#[derive(Clone)]
pub(crate) struct TaskRevisionSource {
    runtime_state: Arc<Mutex<RuntimeState>>,
}

impl TaskRevisionSource {
    pub(crate) fn new(runtime_state: Arc<Mutex<RuntimeState>>) -> Self {
        Self { runtime_state }
    }

    pub(crate) fn current_revision(&self) -> u64 {
        self.runtime_state
            .lock()
            .expect("runtime state poisoned")
            .current_revision()
    }
}
