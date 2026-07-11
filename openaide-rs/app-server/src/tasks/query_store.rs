use crate::diagnostics::TaskDiagnostics;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityToolDetails, MessagePage, TaskSnapshot, TaskSummary};
use crate::storage::Store;
use crate::tasks::snapshot::build_snapshot;

#[derive(Clone)]
pub(crate) struct TaskReadStore {
    store: Store,
}

impl TaskReadStore {
    pub(crate) fn new(store: Store) -> Self {
        Self { store }
    }

    pub(crate) fn list_task_summaries(
        &self,
        archived: bool,
    ) -> Result<Vec<TaskSummary>, RuntimeError> {
        let records = if archived {
            self.store.list_archived_tasks()?
        } else {
            self.store.list_tasks()?
        };
        Ok(records.into_iter().map(|task| task.summary()).collect())
    }

    pub(crate) fn diagnostics(&self, revision: u64) -> Result<TaskDiagnostics, RuntimeError> {
        let visible_tasks = self.store.list_tasks()?;
        Ok(TaskDiagnostics::from_records(
            &visible_tasks,
            self.store.task_record_count()?,
            revision,
        ))
    }

    pub(crate) fn snapshot(
        &self,
        task_id: &str,
        tail_limit: usize,
    ) -> Result<TaskSnapshot, RuntimeError> {
        build_snapshot(&self.store, task_id, tail_limit)
    }

    pub(crate) fn tail_page(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<MessagePage, RuntimeError> {
        self.store.tail_page(task_id, limit)
    }

    pub(crate) fn page_before(
        &self,
        task_id: &str,
        before_cursor: &str,
        limit: usize,
    ) -> Result<MessagePage, RuntimeError> {
        self.store.page_before(task_id, before_cursor, limit)
    }

    pub(crate) fn tool_detail(
        &self,
        task_id: &str,
        artifact_id: &str,
    ) -> Result<ActivityToolDetails, RuntimeError> {
        self.store.read_tool_artifact(task_id, artifact_id)
    }
}
