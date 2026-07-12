use openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot;

use crate::protocol::errors::RuntimeError;

use super::{atomic, Store};

impl Store {
    /// Reads the state-root-wide initial selection offered to clients without a retained choice.
    pub fn read_new_task_defaults(&self) -> Result<NewTaskDefaultsSnapshot, RuntimeError> {
        let path = self.new_task_defaults_path();
        if !path.exists() {
            return Ok(NewTaskDefaultsSnapshot::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    /// Replaces both defaults after the first message promotes a New Task.
    pub fn write_new_task_defaults(
        &self,
        defaults: &NewTaskDefaultsSnapshot,
    ) -> Result<(), RuntimeError> {
        let _guard = self.lock_settings_write();
        atomic::write_json(&self.new_task_defaults_path(), defaults)
    }

    fn new_task_defaults_path(&self) -> std::path::PathBuf {
        self.settings_dir().join("new_task_defaults.json")
    }
}

#[cfg(test)]
#[path = "new_task_defaults_tests.rs"]
mod tests;
