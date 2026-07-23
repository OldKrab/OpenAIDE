use std::sync::{Arc, Mutex};

use crate::diagnostics::TaskDiagnostics;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{MessagePage, TaskSnapshot};
use crate::protocol::params::{ChatPageParams, ChatTailParams, TaskListParams, TaskSnapshotParams};
use crate::protocol::results::TaskListResult;
use crate::tasks::query_store::TaskReadStore;
use crate::tasks::revision_source::TaskRevisionSource;

#[derive(Clone)]
pub(crate) struct TaskQueries {
    store: TaskReadStore,
    store_update_lock: Arc<Mutex<()>>,
    revision_source: TaskRevisionSource,
}

impl TaskQueries {
    pub(crate) fn new(
        store: TaskReadStore,
        store_update_lock: Arc<Mutex<()>>,
        revision_source: TaskRevisionSource,
    ) -> Self {
        Self {
            store,
            store_update_lock,
            revision_source,
        }
    }

    pub(crate) fn list(&self, params: TaskListParams) -> Result<TaskListResult, RuntimeError> {
        let _guard = self.lock();
        Ok(TaskListResult {
            tasks: self.store.list_task_summaries(params.lifecycle)?,
            revision: self.revision_source.current_revision(),
            lifecycle: params.lifecycle,
        })
    }

    pub(crate) fn diagnostics(&self) -> Result<TaskDiagnostics, RuntimeError> {
        let _guard = self.lock();
        self.store
            .diagnostics(self.revision_source.current_revision())
    }

    pub(crate) fn snapshot(
        &self,
        params: TaskSnapshotParams,
    ) -> Result<TaskSnapshot, RuntimeError> {
        let _guard = self.lock();
        self.store.snapshot(&params.task_id, params.tail_limit)
    }

    pub(crate) fn tail(&self, params: ChatTailParams) -> Result<MessagePage, RuntimeError> {
        let _guard = self.lock();
        self.store.tail_page(&params.task_id, params.limit)
    }

    pub(crate) fn page(&self, params: ChatPageParams) -> Result<MessagePage, RuntimeError> {
        let _guard = self.lock();
        self.store
            .page_before(&params.task_id, &params.before_cursor, params.limit)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.store_update_lock
            .lock()
            .expect("store update lock poisoned")
    }
}
