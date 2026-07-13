use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::TaskSetArchivedParams;

use crate::storage::records::TaskLifecycle;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};
use crate::time::now_string;

use super::{conflict_error, protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    pub(super) fn set_task_archived(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetArchivedParams,
    ) -> Result<(), ProtocolError> {
        let task_id = params.task_id.clone();
        let task = self.read_task_for_client(task_id.as_str(), client_instance_id)?;
        if matches!(task.lifecycle, TaskLifecycle::New { .. }) {
            return Err(conflict_error(
                "New Tasks cannot be archived; discard it explicitly instead",
            ));
        }
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

        Ok(())
    }
}
