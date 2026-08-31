use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;

use super::{atomic, Store};

/// Settings owned by the App Server runtime rather than a particular shell.
/// Keeping this file independent from the wire protocol lets every shell share
/// the same persisted value and keeps protocol additions backwards compatible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredRuntimeSettings {
    #[serde(default)]
    acp_trace_enabled: bool,
}

impl Store {
    /// Reads the App Server-wide ACP trace toggle, defaulting off for older roots.
    pub fn read_acp_trace_enabled(&self) -> Result<bool, RuntimeError> {
        let path = self.runtime_settings_path();
        if !path.exists() {
            return Ok(false);
        }
        let settings: StoredRuntimeSettings =
            serde_json::from_str(&std::fs::read_to_string(path)?)?;
        Ok(settings.acp_trace_enabled)
    }

    /// Atomically persists the App Server-wide ACP trace toggle.
    pub fn write_acp_trace_enabled(&self, enabled: bool) -> Result<(), RuntimeError> {
        let _guard = self.lock_settings_write();
        atomic::write_json(
            &self.runtime_settings_path(),
            &StoredRuntimeSettings {
                acp_trace_enabled: enabled,
            },
        )
    }

    fn runtime_settings_path(&self) -> std::path::PathBuf {
        self.settings_dir().join("runtime_settings.json")
    }
}

#[cfg(test)]
#[path = "runtime_settings_tests.rs"]
mod tests;
