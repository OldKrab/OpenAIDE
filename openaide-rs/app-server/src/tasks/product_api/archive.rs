use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskNavigationSnapshot;
use openaide_app_server_protocol::task::TaskSetArchivedParams;

use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};
use crate::time::now_string;

use super::{protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    pub(super) fn set_task_archived(
        &self,
        params: TaskSetArchivedParams,
    ) -> Result<TaskNavigationSnapshot, ProtocolError> {
        let task_id = params.task_id.clone();
        let now = now_string();
        self.mutations
            .commit_existing_task(task_id.as_str(), TaskCommitOptions::metadata(), |ctx| {
                if ctx.task().archived == params.archived {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                task.archived = params.archived;
                task.updated_at = now;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;

        let tasks = if params.archived {
            self.store.list_tasks()
        } else {
            self.store.list_archived_tasks()
        }
        .map_err(protocol_error_from_runtime)?
        .into_iter()
        .map(crate::snapshots::project_task_summary)
        .collect();
        Ok(TaskNavigationSnapshot {
            tasks,
            active_task_id: None,
        })
    }
}
