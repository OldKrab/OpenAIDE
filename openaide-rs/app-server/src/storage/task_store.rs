use std::fs;

use crate::protocol::errors::RuntimeError;

use super::atomic;
use super::records::TaskRecord;
use super::Store;

impl Store {
    pub fn write_task(&self, record: &TaskRecord) -> Result<(), RuntimeError> {
        #[cfg(test)]
        if self.take_task_write_failure_for_test() {
            return Err(RuntimeError::Storage(
                "injected Task record write failure".to_string(),
            ));
        }
        atomic::write_json(&self.task_dir(&record.task_id)?.join("task.json"), record)
    }

    pub fn read_task(&self, task_id: &str) -> Result<TaskRecord, RuntimeError> {
        let path = self.task_dir(task_id)?.join("task.json");
        if !path.exists() {
            return Err(RuntimeError::TaskNotFound(task_id.to_string()));
        }
        let text = fs::read_to_string(path)?;
        let task = serde_json::from_str(&text)?;
        #[cfg(test)]
        self.run_after_task_read_hook_for_test();
        Ok(task)
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived(false)
    }

    pub(crate) fn list_tasks_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived_strict(false)
    }

    pub fn list_archived_tasks(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived(true)
    }

    pub(crate) fn list_archived_tasks_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived_strict(true)
    }

    pub fn list_all_task_records(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_any_task_record_from_entry(entry?)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    pub(crate) fn list_all_task_records_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_any_task_record_from_entry_strict(entry?)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    fn list_tasks_by_archived(&self, archived: bool) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_task_record_from_entry(entry?, archived)? {
                records.push(record);
            }
        }
        records.sort_by(compare_task_records_for_navigation);
        Ok(records)
    }

    fn list_tasks_by_archived_strict(
        &self,
        archived: bool,
    ) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_task_record_from_entry_strict(entry?, archived)? {
                records.push(record);
            }
        }
        records.sort_by(compare_task_records_for_navigation);
        Ok(records)
    }

    pub fn max_task_revision(&self) -> Result<u64, RuntimeError> {
        let mut revision = 0;
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_any_task_record_from_entry(entry?)? {
                revision = revision.max(record.revision);
            }
        }
        Ok(revision)
    }

    /// Returns the collection revision without exposing client-private New Task activity.
    pub(crate) fn max_visible_task_revision(&self) -> Result<u64, RuntimeError> {
        Ok(self
            .list_all_task_records_strict()?
            .into_iter()
            .filter(|task| task.lifecycle.is_visible())
            .map(|task| task.revision)
            .max()
            .unwrap_or(0))
    }

    pub fn task_record_count(&self) -> Result<usize, RuntimeError> {
        let mut count = 0;
        for entry in fs::read_dir(self.tasks_dir())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && entry.path().join("task.json").exists() {
                count += 1;
            }
        }
        Ok(count)
    }
}

fn compare_task_records_for_navigation(
    left: &TaskRecord,
    right: &TaskRecord,
) -> std::cmp::Ordering {
    compare_desc(&left.last_activity, &right.last_activity)
        .then_with(|| right.last_activity.cmp(&left.last_activity))
        .then_with(|| right.task_id.cmp(&left.task_id))
}

fn compare_desc(left: &str, right: &str) -> std::cmp::Ordering {
    crate::time::activity_millis(right)
        .unwrap_or(0)
        .cmp(&crate::time::activity_millis(left).unwrap_or(0))
}

fn read_task_record_from_entry(
    entry: fs::DirEntry,
    archived_filter: bool,
) -> Result<Option<TaskRecord>, RuntimeError> {
    let Some(record) = read_any_task_record_from_entry(entry)? else {
        return Ok(None);
    };
    if record.tombstoned || !record.lifecycle.is_visible() || record.archived != archived_filter {
        return Ok(None);
    }
    Ok(Some(record))
}

fn read_task_record_from_entry_strict(
    entry: fs::DirEntry,
    archived_filter: bool,
) -> Result<Option<TaskRecord>, RuntimeError> {
    let Some(record) = read_any_task_record_from_entry_strict(entry)? else {
        return Ok(None);
    };
    if record.tombstoned || !record.lifecycle.is_visible() || record.archived != archived_filter {
        return Ok(None);
    }
    Ok(Some(record))
}

fn read_any_task_record_from_entry(
    entry: fs::DirEntry,
) -> Result<Option<TaskRecord>, RuntimeError> {
    if !entry.file_type()?.is_dir() {
        return Ok(None);
    }
    let path = entry.path().join("task.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    match serde_json::from_str(&text) {
        Ok(record) => Ok(Some(record)),
        Err(_) => Ok(None),
    }
}

fn read_any_task_record_from_entry_strict(
    entry: fs::DirEntry,
) -> Result<Option<TaskRecord>, RuntimeError> {
    if !entry.file_type()?.is_dir() {
        return Ok(None);
    }
    let path = entry.path().join("task.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}
