use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Wakes transport links when App Server state that may produce client
/// deliveries has changed. It carries no payload: a woken link drains its own
/// connection, so a spurious wake costs one empty drain and a coalesced wake
/// loses nothing.
#[derive(Clone, Debug, Default)]
pub struct DeliverySignal {
    inner: Arc<(Mutex<u64>, Condvar)>,
}

impl DeliverySignal {
    /// Snapshot taken before a drain; a change after it means "drain again".
    pub fn generation(&self) -> u64 {
        *self.inner.0.lock().expect("delivery signal poisoned")
    }

    pub fn notify(&self) {
        let mut generation = self.inner.0.lock().expect("delivery signal poisoned");
        *generation = generation.wrapping_add(1);
        self.inner.1.notify_all();
    }

    /// Blocks until the generation differs from `seen` or the timeout elapses.
    /// The timeout bounds liveness renewal and recovers from any writer that
    /// bypasses the signal.
    pub fn wait_changed(&self, seen: u64, timeout: Duration) -> u64 {
        let generation = self.inner.0.lock().expect("delivery signal poisoned");
        let (generation, _) = self
            .inner
            .1
            .wait_timeout_while(generation, timeout, |current| *current == seen)
            .expect("delivery signal poisoned");
        *generation
    }
}

#[cfg(test)]
#[path = "delivery_signal_tests.rs"]
mod tests;
