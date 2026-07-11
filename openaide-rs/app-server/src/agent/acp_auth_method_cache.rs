use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(super) struct AcpAuthMethodCache {
    inner: Arc<Mutex<Option<String>>>,
}

impl AcpAuthMethodCache {
    pub(super) fn record_authenticated_method(&self, method_id: String) {
        *self.inner.lock().expect("ACP auth method cache poisoned") = Some(method_id);
    }

    pub(super) fn preferred_method(&self) -> Option<String> {
        self.inner
            .lock()
            .expect("ACP auth method cache poisoned")
            .clone()
    }
}
