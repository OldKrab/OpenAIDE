use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::storage::{atomic, Store};

const CATALOG_VERSION: u32 = 1;

/// Stable Agent-scoped identity for one Agent-owned conversation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSessionRef {
    pub(crate) agent_id: String,
    pub(crate) session_id: String,
}

impl NativeSessionRef {
    pub(crate) fn new(agent_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            session_id: session_id.into(),
        }
    }
}

/// Metadata observed through `session/list`; absence never clears a known value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSessionObservation {
    pub(crate) reference: NativeSessionRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_activity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSessionCatalogEntry {
    pub(crate) project_id: String,
    pub(crate) workspace_root: String,
    #[serde(flatten)]
    pub(crate) observation: NativeSessionObservation,
}

/// Durable Native Session listing observations. ACP cursors and refresh generations stay in memory.
#[derive(Clone)]
pub(crate) struct NativeSessionCatalog {
    store: Store,
    state: Arc<Mutex<StoredNativeSessionCatalog>>,
    refresh_state: Arc<Mutex<openaide_app_server_protocol::snapshot::TaskNavigationRefreshState>>,
    projects_with_more: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl NativeSessionCatalog {
    pub(crate) fn open(store: Store) -> Result<Self, RuntimeError> {
        let path = catalog_path(&store);
        let state = if path.exists() {
            let text = std::fs::read_to_string(path)?;
            serde_json::from_str(&text)?
        } else {
            StoredNativeSessionCatalog::default()
        };
        Ok(Self {
            store,
            state: Arc::new(Mutex::new(state)),
            refresh_state: Arc::new(Mutex::new(
                openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Idle,
            )),
            projects_with_more: Arc::new(Mutex::new(std::collections::HashSet::new())),
        })
    }

    /// Commits one successful page without interpreting omitted sessions as deletion.
    pub(crate) fn record_page(
        &self,
        project_id: &str,
        workspace_root: &str,
        observations: Vec<NativeSessionObservation>,
    ) -> Result<(), RuntimeError> {
        let mut state = self.state.lock().expect("native session catalog poisoned");
        for observation in observations {
            if let Some(existing) = state
                .entries
                .iter_mut()
                .find(|entry| entry.observation.reference == observation.reference)
            {
                if existing.workspace_root != workspace_root {
                    crate::logging::warn(
                        "native_session_catalog_workspace_conflict",
                        serde_json::json!({
                            "agent_id": observation.reference.agent_id,
                            "session_id": observation.reference.session_id,
                        }),
                    );
                    continue;
                }
                let advances_activity = observation
                    .last_activity
                    .as_deref()
                    .and_then(crate::time::activity_millis)
                    .zip(
                        existing
                            .observation
                            .last_activity
                            .as_deref()
                            .and_then(crate::time::activity_millis),
                    )
                    .is_some_and(|(incoming, current)| incoming > current)
                    || (observation.last_activity.is_some()
                        && existing.observation.last_activity.is_none());
                // A listing page may lag behind a live session_info_update. Only newer
                // activity evidence may replace metadata already held by the catalog.
                if advances_activity {
                    existing.observation.last_activity = observation.last_activity;
                    if observation.title.is_some() {
                        existing.observation.title = observation.title;
                    }
                } else if existing.observation.title.is_none() && observation.title.is_some() {
                    existing.observation.title = observation.title;
                }
                continue;
            }
            state.entries.push(NativeSessionCatalogEntry {
                project_id: project_id.to_string(),
                workspace_root: workspace_root.to_string(),
                observation,
            });
        }
        atomic::write_json(&catalog_path(&self.store), &*state)
    }

    /// Merges authoritative metadata observed from an already-live Native Session.
    /// `title` distinguishes an omitted field from an explicit clear.
    pub(crate) fn record_live_metadata(
        &self,
        reference: &NativeSessionRef,
        title: Option<Option<String>>,
        updated_at: Option<String>,
    ) -> Result<bool, RuntimeError> {
        let mut state = self.state.lock().expect("native session catalog poisoned");
        let Some(existing) = state
            .entries
            .iter_mut()
            .find(|entry| &entry.observation.reference == reference)
        else {
            return Ok(false);
        };
        let mut changed = false;
        if let Some(title) = title {
            if existing.observation.title != title {
                existing.observation.title = title;
                changed = true;
            }
        }
        if let Some(updated_at) = updated_at {
            let advances_activity = crate::time::activity_millis(&updated_at)
                .zip(
                    existing
                        .observation
                        .last_activity
                        .as_deref()
                        .and_then(crate::time::activity_millis),
                )
                .is_some_and(|(incoming, current)| incoming > current)
                || existing.observation.last_activity.is_none();
            if advances_activity {
                existing.observation.last_activity = Some(updated_at);
                changed = true;
            }
        }
        if changed {
            atomic::write_json(&catalog_path(&self.store), &*state)?;
        }
        Ok(changed)
    }

    #[cfg(test)]
    pub(crate) fn project(&self, project_id: &str) -> Vec<NativeSessionObservation> {
        self.project_entries(project_id)
            .into_iter()
            .map(|entry| entry.observation)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn project_entries(&self, project_id: &str) -> Vec<NativeSessionCatalogEntry> {
        self.state
            .lock()
            .expect("native session catalog poisoned")
            .entries
            .iter()
            .filter(|entry| entry.project_id == project_id)
            .cloned()
            .collect()
    }

    pub(crate) fn entries(&self) -> Vec<NativeSessionCatalogEntry> {
        self.state
            .lock()
            .expect("native session catalog poisoned")
            .entries
            .clone()
    }

    pub(crate) fn entry(&self, reference: &NativeSessionRef) -> Option<NativeSessionCatalogEntry> {
        self.state
            .lock()
            .expect("native session catalog poisoned")
            .entries
            .iter()
            .find(|entry| &entry.observation.reference == reference)
            .cloned()
    }

    /// Removes only after a definitive `session/load` failure; partial listings never delete.
    pub(crate) fn remove(&self, reference: &NativeSessionRef) -> Result<bool, RuntimeError> {
        let mut state = self.state.lock().expect("native session catalog poisoned");
        let before = state.entries.len();
        state
            .entries
            .retain(|entry| &entry.observation.reference != reference);
        if state.entries.len() == before {
            return Ok(false);
        }
        atomic::write_json(&catalog_path(&self.store), &*state)?;
        Ok(true)
    }

    pub(crate) fn set_refreshing(&self, refreshing: bool) {
        self.set_refresh_state(if refreshing {
            openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Refreshing
        } else {
            openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Idle
        });
    }

    #[cfg(test)]
    pub(crate) fn refreshing(&self) -> bool {
        matches!(
            self.refresh_state(),
            openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Refreshing
        )
    }

    pub(crate) fn set_refresh_state(
        &self,
        state: openaide_app_server_protocol::snapshot::TaskNavigationRefreshState,
    ) {
        *self
            .refresh_state
            .lock()
            .expect("native session refresh state poisoned") = state;
    }

    pub(crate) fn refresh_state(
        &self,
    ) -> openaide_app_server_protocol::snapshot::TaskNavigationRefreshState {
        self.refresh_state
            .lock()
            .expect("native session refresh state poisoned")
            .clone()
    }

    /// Records whether discovery stopped before exhausting a Project's Agent history.
    pub(crate) fn set_project_has_more(&self, project_id: &str, has_more: bool) -> bool {
        let mut projects = self
            .projects_with_more
            .lock()
            .expect("native session pagination state poisoned");
        if has_more {
            projects.insert(project_id.to_string())
        } else {
            projects.remove(project_id)
        }
    }

    pub(crate) fn project_has_more(&self, project_id: &str) -> bool {
        self.projects_with_more
            .lock()
            .expect("native session pagination state poisoned")
            .contains(project_id)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredNativeSessionCatalog {
    version: u32,
    entries: Vec<NativeSessionCatalogEntry>,
}

impl Default for StoredNativeSessionCatalog {
    fn default() -> Self {
        Self {
            version: CATALOG_VERSION,
            entries: Vec::new(),
        }
    }
}

fn catalog_path(store: &Store) -> std::path::PathBuf {
    store.root().join("native-sessions").join("catalog.json")
}
