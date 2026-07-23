use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::{
    TaskArchiveParams, TaskLifecycleChanged, TaskRestoreParams,
};

use crate::protocol::model::TaskStatus;
use crate::snapshots::project_task_summary;
use crate::storage::records::TaskLifecycle;
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::{conflict_error, protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    pub(super) fn archive_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskArchiveParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        let task_id = params.task_id.clone();
        let task = self.read_task_for_client(task_id.as_str(), client_instance_id)?;
        if matches!(task.lifecycle, TaskLifecycle::Prepared { .. }) {
            return Err(conflict_error(
                "Prepared Tasks cannot be archived; discard it explicitly instead",
            ));
        }
        if matches!(task.lifecycle, TaskLifecycle::Archived) {
            return Ok(lifecycle_change(&task, TaskLifecycle::Archived));
        }
        if task.active_turn_id.is_some()
            || matches!(
                task.status,
                TaskStatus::Starting | TaskStatus::Active | TaskStatus::Stopping
            )
            || self.server_requests.has_pending_for_task(&task_id)
        {
            return Err(conflict_error(
                "A Task must be idle before it can be archived",
            ));
        }

        self.transition_task_lifecycle(task, TaskLifecycle::Archived)
    }

    pub(super) fn restore_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskRestoreParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        let task = self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        if matches!(task.lifecycle, TaskLifecycle::Prepared { .. }) {
            return Err(conflict_error("Prepared Tasks cannot be restored"));
        }
        if matches!(task.lifecycle, TaskLifecycle::Open) {
            return Ok(lifecycle_change(&task, TaskLifecycle::Open));
        }
        self.transition_task_lifecycle(task, TaskLifecycle::Open)
    }

    fn transition_task_lifecycle(
        &self,
        task: crate::storage::records::TaskRecord,
        next_lifecycle: TaskLifecycle,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        let task_id = task.task_id.clone();
        let previous_lifecycle = task.lifecycle.clone();
        let now = now_string();
        let result = self
            .mutations
            .commit_existing_task(task_id.as_str(), TaskCommitOptions::metadata(), |ctx| {
                let task = ctx.task_mut();
                task.lifecycle = next_lifecycle.clone();
                if matches!(next_lifecycle, TaskLifecycle::Archived) {
                    task.clear_process_local_agent_state();
                }
                task.updated_at = now;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let TaskCommitOutcome::Committed(facts) = result.outcome else {
            return Err(conflict_error("Task lifecycle did not change"));
        };
        Ok(lifecycle_change(&facts.committed_task, previous_lifecycle))
    }
}

fn lifecycle_change(
    task: &crate::storage::records::TaskRecord,
    previous_lifecycle: TaskLifecycle,
) -> TaskLifecycleChanged {
    TaskLifecycleChanged {
        previous_lifecycle: crate::snapshots::project_task_lifecycle(&previous_lifecycle),
        task: project_task_summary(task.clone()),
    }
}
