//! The attachment's latest complete session snapshot, readable without its event loop.
//!
//! The event loop stays the only writer: it publishes after it applies an update, so a
//! reader sees the same state a queued snapshot request would have returned. Reading
//! never waits for the loop, which may be busy delivering updates to a slow sink.

use tokio::sync::watch;

use crate::agent::acp_session_catalogs::session_with_catalog_snapshots;
use crate::agent::AgentSession;
use crate::protocol::model::{AgentCommandsCatalog, ConfigOptionsCatalog};

/// Creates the channel before the session opens; the opener publishes the started
/// session before it reports startup, so no reader observes the empty value.
pub(in crate::agent) fn session_snapshot_channel(
) -> (SessionSnapshotPublisher, SessionSnapshotReader) {
    let (tx, rx) = watch::channel(None);
    (SessionSnapshotPublisher(tx), SessionSnapshotReader(rx))
}

/// Write side, owned by the opener and then by the attachment's event loop.
pub(in crate::agent) struct SessionSnapshotPublisher(watch::Sender<Option<AgentSession>>);

impl SessionSnapshotPublisher {
    /// Replaces the snapshot after an open or a load establishes a new session state.
    pub(in crate::agent) fn publish(&self, session: &AgentSession) {
        self.0.send_replace(Some(session.clone()));
    }

    /// Folds the loop's current catalogs into the snapshot, cloning only on a change.
    pub(in crate::agent) fn publish_catalogs(
        &self,
        config_catalog: &ConfigOptionsCatalog,
        commands_catalog: &Option<AgentCommandsCatalog>,
    ) {
        self.0.send_if_modified(|published| {
            let Some(session) = published else {
                return false;
            };
            // An empty agent id marks a catalog the Agent never reported; see
            // `session_with_catalog_snapshots`.
            let config_changed = !config_catalog.agent_id.is_empty()
                && session.config_catalog.as_ref() != Some(config_catalog);
            if !config_changed && session.commands_catalog == *commands_catalog {
                return false;
            }
            *session = session_with_catalog_snapshots(session, config_catalog, commands_catalog);
            true
        });
    }
}

/// Read side, held by the attachment handle.
#[derive(Clone)]
pub(in crate::agent) struct SessionSnapshotReader(watch::Receiver<Option<AgentSession>>);

impl SessionSnapshotReader {
    /// Returns the latest snapshot, or `None` once the event loop has stopped.
    pub(in crate::agent) fn read(&self) -> Option<AgentSession> {
        // The publisher drops with the event loop, like the command channel did.
        self.0.has_changed().ok()?;
        self.0.borrow().clone()
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
