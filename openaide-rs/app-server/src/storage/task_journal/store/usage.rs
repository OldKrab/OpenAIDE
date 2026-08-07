use std::fs;

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;

use super::TaskJournalStore;

const LAST_USED_FILE: &str = "last-used.ms";

impl TaskJournalStore {
    /// Records client use separately from semantic Task history, avoiding a
    /// full metadata journal revision for every open.
    pub(crate) fn mark_task_used(
        &self,
        task_id: &str,
        used_at_millis: i128,
    ) -> Result<(), RuntimeError> {
        validate_task_id(task_id)?;
        if used_at_millis < 0 {
            return Err(RuntimeError::InvalidParams("used_at_millis".to_string()));
        }
        let task = self
            .inner
            .catalog
            .read()
            .expect("Task catalog poisoned")
            .get(task_id)
            .cloned()
            .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
        if task.tombstoned {
            return Err(RuntimeError::TaskNotFound(task_id.to_string()));
        }
        if self
            .task_last_used_millis(task_id)?
            .is_some_and(|current| current >= used_at_millis)
        {
            return Ok(());
        }
        let path = self.inner.tasks_root.join(task_id).join(LAST_USED_FILE);
        crate::storage::atomic::write_bytes(&path, format!("{used_at_millis}\n").as_bytes())
    }

    pub(crate) fn task_last_used_millis(
        &self,
        task_id: &str,
    ) -> Result<Option<i128>, RuntimeError> {
        validate_task_id(task_id)?;
        let path = self.inner.tasks_root.join(task_id).join(LAST_USED_FILE);
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let used_at = contents.trim().parse::<i128>().map_err(|_| {
            // Invalid markers fail closed: maintenance retains the Task until
            // a later open repairs its tiny usage record.
            RuntimeError::Storage("Task last-used marker is invalid".to_string())
        })?;
        if used_at < 0 {
            return Err(RuntimeError::Storage(
                "Task last-used marker is invalid".to_string(),
            ));
        }
        Ok(Some(used_at))
    }
}
