//! Command catalogs an Agent publishes for a session this client is still opening.
//!
//! An Agent may send a session's first command catalog before the response that names
//! the session is processed. No session handler exists yet, so the ACP dispatch loop
//! would drop the update and the Task would show no commands until the Agent repeats it.
//! The loop can instead defer an update until a handler accepts it, which delivers it to
//! the session exactly once. It keeps a deferred update no handler ever accepts, so this
//! module decides which updates may be deferred and bounds what can be left behind.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::logging;

/// Deferred updates the connection may leave unclaimed over its lifetime.
const UNCLAIMED_UPDATE_LIMIT: usize = 16;

/// Shared by one Agent connection's notification handler and its session opens.
#[derive(Clone, Default)]
pub(super) struct EarlySessionCatalogs {
    state: Arc<Mutex<EarlySessionCatalogState>>,
}

#[derive(Default)]
struct EarlySessionCatalogState {
    opens_in_flight: usize,
    /// Updates deferred during the current opens, by session.
    deferred: HashMap<String, usize>,
    unclaimed: usize,
}

impl EarlySessionCatalogs {
    /// Starts the window in which an unattached session's catalog may be deferred.
    pub(super) fn begin_open(&self) -> EarlySessionCatalogOpen {
        self.lock().opens_in_flight += 1;
        EarlySessionCatalogOpen {
            catalogs: self.clone(),
        }
    }

    /// Decides whether the dispatch loop should defer this session's catalog update.
    /// Only a session being opened can claim one, and only while the budget lasts.
    pub(super) fn defer_commands(&self, session_id: &str) -> bool {
        let mut state = self.lock();
        let pending: usize = state.deferred.values().sum();
        if state.opens_in_flight == 0 || state.unclaimed + pending >= UNCLAIMED_UPDATE_LIMIT {
            return false;
        }
        *state.deferred.entry(session_id.to_string()).or_default() += 1;
        true
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, EarlySessionCatalogState> {
        self.state
            .lock()
            .expect("ACP early session catalog lock poisoned")
    }
}

/// One session open. Dropping it ends the deferral window.
pub(super) struct EarlySessionCatalogOpen {
    catalogs: EarlySessionCatalogs,
}

impl EarlySessionCatalogOpen {
    /// Ends the open for a session whose handler now exists and received its updates.
    pub(super) fn finish(self, session_id: &str) {
        self.catalogs.lock().deferred.remove(session_id);
    }
}

impl Drop for EarlySessionCatalogOpen {
    fn drop(&mut self) {
        let (unclaimed, total) = {
            let mut state = self.catalogs.lock();
            state.opens_in_flight = state.opens_in_flight.saturating_sub(1);
            if state.opens_in_flight > 0 {
                return;
            }
            let unclaimed: usize = std::mem::take(&mut state.deferred).values().sum();
            state.unclaimed += unclaimed;
            (unclaimed, state.unclaimed)
        };
        if unclaimed > 0 {
            logging::warn(
                "acp_session_early_catalog_unclaimed",
                serde_json::json!({ "count": unclaimed, "retained_count": total }),
            );
        }
    }
}

#[cfg(test)]
#[path = "acp_early_session_catalogs_tests.rs"]
mod tests;
