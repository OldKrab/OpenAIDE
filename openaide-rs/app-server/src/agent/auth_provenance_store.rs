use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::storage::{atomic, Store};

const SCHEMA_VERSION: u32 = 1;
const FILE_NAME: &str = "auth-provenance.json";

/// Persists only the last explicitly successful method id used to target credential cleanup.
#[derive(Clone)]
pub(super) struct AgentAuthProvenanceStore {
    store: Store,
    /// Authentication requests may run concurrently for different Agents; serialize the
    /// load-modify-save sequence so one successful method cannot erase another.
    access: Arc<Mutex<()>>,
}

impl AgentAuthProvenanceStore {
    pub(super) fn new(store: Store) -> Self {
        Self {
            store,
            access: Arc::new(Mutex::new(())),
        }
    }

    pub(super) fn method(&self, agent_id: &str) -> Result<Option<String>, RuntimeError> {
        let _access = self
            .access
            .lock()
            .expect("Agent auth provenance lock poisoned");
        Ok(self.load()?.methods.get(agent_id).cloned())
    }

    pub(super) fn record(&self, agent_id: &str, method_id: &str) -> Result<(), RuntimeError> {
        let _access = self
            .access
            .lock()
            .expect("Agent auth provenance lock poisoned");
        let mut state = self.load()?;
        state
            .methods
            .insert(agent_id.to_string(), method_id.to_string());
        self.save(&state)
    }

    pub(super) fn clear(&self, agent_id: &str) -> Result<(), RuntimeError> {
        let _access = self
            .access
            .lock()
            .expect("Agent auth provenance lock poisoned");
        let mut state = self.load()?;
        if state.methods.remove(agent_id).is_some() {
            self.save(&state)?;
        }
        Ok(())
    }

    fn load(&self) -> Result<StoredAuthProvenance, RuntimeError> {
        let path = self.path();
        if !path.exists() {
            return Ok(StoredAuthProvenance::default());
        }
        let state: StoredAuthProvenance = serde_json::from_slice(&std::fs::read(path)?)?;
        if state.schema_version != SCHEMA_VERSION {
            return Err(RuntimeError::Storage(format!(
                "unsupported Agent auth provenance schema {}",
                state.schema_version
            )));
        }
        Ok(state)
    }

    fn save(&self, state: &StoredAuthProvenance) -> Result<(), RuntimeError> {
        atomic::write_json(&self.path(), state)
    }

    fn path(&self) -> std::path::PathBuf {
        self.store.agents_dir().join(FILE_NAME)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAuthProvenance {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(default)]
    methods: BTreeMap<String, String>,
}

impl Default for StoredAuthProvenance {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            methods: BTreeMap::new(),
        }
    }
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}
