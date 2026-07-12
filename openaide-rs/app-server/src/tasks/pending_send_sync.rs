use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::tasks::product_api::send::PendingSendSync;

#[cfg(test)]
type PendingSendTestHook = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

/// Owns accepted sends whose Native Session history must be retried before prompting.
///
/// History snapshot projection deliberately does not own this payload. Cancel,
/// retry, and support recovery all consume the same exact accepted send.
#[derive(Clone, Default)]
pub(super) struct PendingSendSyncCoordinator {
    sends: Arc<Mutex<HashMap<String, PendingSendSync>>>,
    #[cfg(test)]
    before_next_defer: PendingSendTestHook,
    #[cfg(test)]
    after_next_defer: PendingSendTestHook,
}

impl PendingSendSyncCoordinator {
    pub(super) fn defer(&self, task_id: &str, pending: PendingSendSync) {
        #[cfg(test)]
        run_test_hook(&self.before_next_defer);
        self.sends
            .lock()
            .expect("pending send synchronization registry poisoned")
            .insert(task_id.to_string(), pending);
        #[cfg(test)]
        run_test_hook(&self.after_next_defer);
    }

    pub(super) fn take(&self, task_id: &str) -> Option<PendingSendSync> {
        self.sends
            .lock()
            .expect("pending send synchronization registry poisoned")
            .remove(task_id)
    }

    pub(super) fn contains(&self, task_id: &str) -> bool {
        self.sends
            .lock()
            .expect("pending send synchronization registry poisoned")
            .contains_key(task_id)
    }

    #[cfg(test)]
    pub(super) fn before_next_defer_for_test(&self, hook: impl FnOnce() + Send + 'static) {
        *self
            .before_next_defer
            .lock()
            .expect("pending send synchronization test hook poisoned") = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(super) fn after_next_defer_for_test(&self, hook: impl FnOnce() + Send + 'static) {
        *self
            .after_next_defer
            .lock()
            .expect("pending send synchronization test hook poisoned") = Some(Box::new(hook));
    }
}

#[cfg(test)]
fn run_test_hook(hook: &PendingSendTestHook) {
    let hook = hook
        .lock()
        .expect("pending send synchronization test hook poisoned")
        .take();
    if let Some(hook) = hook {
        hook();
    }
}
