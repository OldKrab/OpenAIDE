use crate::protocol::errors::RuntimeError;

use super::records::{MessageMeta, TaskRecord};
use super::task_journal::{TaskProjection, TaskWrite};
use super::Store;

impl Store {
    /// Commits a Task record through the canonical journal owner.
    ///
    /// Production workflows stage Task and Chat together; this compatibility
    /// surface remains for non-workflow callers and focused storage fixtures.
    pub fn write_task(&self, record: &TaskRecord) -> Result<(), RuntimeError> {
        #[cfg(test)]
        if self.take_task_write_failure_for_test() {
            return Err(RuntimeError::Storage(
                "injected Task record write failure".to_string(),
            ));
        }
        let write = match self.task_journal().load(&record.task_id) {
            Ok(_) => TaskWrite::barrier_replace_task(record.clone()),
            Err(RuntimeError::TaskNotFound(_)) => TaskWrite::barrier_create(TaskProjection {
                task: record.clone(),
                messages: Vec::new(),
                message_meta: MessageMeta {
                    task_id: record.task_id.clone(),
                    ..MessageMeta::default()
                },
                artifact_heads: Default::default(),
            }),
            Err(error) => return Err(error),
        };
        self.task_journal().submit(write)?.wait()?;
        Ok(())
    }

    pub fn read_task(&self, task_id: &str) -> Result<TaskRecord, RuntimeError> {
        let task = self.task_journal().load(task_id)?.task;
        #[cfg(test)]
        self.run_after_task_read_hook_for_test();
        Ok(task)
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived(false)
    }

    pub fn list_archived_tasks(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived(true)
    }

    pub fn list_all_task_records(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        Ok(self
            .task_journal()
            .list_task_records()
            .into_iter()
            .collect())
    }

    pub(crate) fn list_all_task_records_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        Ok(self
            .task_journal()
            .list_task_records_strict()?
            .into_iter()
            .collect())
    }

    fn list_tasks_by_archived(&self, archived: bool) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = self
            .task_journal()
            .list_task_records()
            .into_iter()
            .filter(|record| {
                !record.tombstoned && record.lifecycle.is_visible() && record.archived == archived
            })
            .collect::<Vec<_>>();
        records.sort_by(compare_task_records_for_navigation);
        Ok(records)
    }

    pub fn max_task_revision(&self) -> Result<u64, RuntimeError> {
        Ok(self
            .task_journal()
            .list_task_records()
            .into_iter()
            .map(|task| task.revision)
            .max()
            .unwrap_or(0))
    }

    /// Returns the collection revision without exposing client-private New Task activity.
    pub(crate) fn max_visible_task_revision(&self) -> Result<u64, RuntimeError> {
        Ok(self
            .list_all_task_records()?
            .into_iter()
            .filter(|task| task.lifecycle.is_visible())
            .map(|task| task.revision)
            .max()
            .unwrap_or(0))
    }

    pub fn task_record_count(&self) -> Result<usize, RuntimeError> {
        Ok(self.task_journal().list_task_records().len())
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
