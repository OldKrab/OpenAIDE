use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::TaskSummary;
use openaide_app_server_protocol::task::TaskSetPinnedParams;

use crate::snapshots::project_task_summary;
use crate::storage::records::TaskLifecycle;
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::{conflict_error, protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    /// Persists one user-owned navigation preference without fabricating Task activity.
    pub(super) fn set_task_pinned(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetPinnedParams,
    ) -> Result<TaskSummary, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let now = now_string();
        let result = self
            .mutations
            .commit_existing_task(
                params.task_id.as_str(),
                TaskCommitOptions::metadata(),
                |ctx| {
                    let task = ctx.task_mut();
                    match task.lifecycle {
                        TaskLifecycle::Open => {}
                        TaskLifecycle::Prepared { .. } => {
                            return Err(crate::protocol::errors::RuntimeError::Conflict(
                                "Prepared Tasks cannot be pinned".to_string(),
                            ));
                        }
                        TaskLifecycle::Archived => {
                            return Err(crate::protocol::errors::RuntimeError::Conflict(
                                "Archived Tasks are read-only; restore the Task before pinning it"
                                    .to_string(),
                            ));
                        }
                    }
                    if task.pinned == params.pinned {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    task.pinned = params.pinned;
                    task.updated_at = now;
                    Ok(TaskMutationResult::Changed)
                },
            )
            .map_err(protocol_error_from_runtime)?;

        match result.outcome {
            TaskCommitOutcome::Committed(facts) => {
                crate::logging::info(
                    "task_pin_changed",
                    serde_json::json!({
                        "task_id": params.task_id,
                        "pinned": params.pinned,
                    }),
                );
                Ok(project_task_summary(facts.committed_task))
            }
            TaskCommitOutcome::Rejected(_) => {
                let current =
                    self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
                if !matches!(current.lifecycle, TaskLifecycle::Open) {
                    return Err(conflict_error("Only Open Tasks can be pinned"));
                }
                Ok(project_task_summary(current))
            }
        }
    }
}
