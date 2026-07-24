use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::TaskSummary;
use openaide_app_server_protocol::task::{TaskSetTitleParams, TaskTitleSelection};

use crate::snapshots::project_task_summary;
use crate::storage::records::TaskLifecycle;
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::{conflict_error, protocol_error_from_runtime, validation_error, TaskProductApi};

const MAX_USER_TITLE_CHARS: usize = 200;

impl TaskProductApi {
    pub(super) fn set_task_title(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetTitleParams,
    ) -> Result<TaskSummary, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let selection = normalize_selection(params.title)?;
        let now = now_string();
        let result = self
            .mutations
            .commit_existing_task(
                params.task_id.as_str(),
                TaskCommitOptions::metadata(),
                |ctx| {
                    let task = ctx.task_mut();
                    match task.lifecycle {
                        TaskLifecycle::Prepared { .. } => {
                            return Err(crate::protocol::errors::RuntimeError::Conflict(
                                "Prepared Tasks cannot be renamed".to_string(),
                            ));
                        }
                        TaskLifecycle::Archived => {
                            return Err(crate::protocol::errors::RuntimeError::Conflict(
                                "Archived Tasks are read-only; restore the Task before renaming it"
                                    .to_string(),
                            ));
                        }
                        TaskLifecycle::Open => {}
                    }
                    let changed = match &selection {
                        TaskTitleSelection::User { value } => {
                            task.title.set_user_title(value.clone())
                        }
                        TaskTitleSelection::Automatic => task.title.reset_to_automatic(),
                    };
                    if !changed {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    task.updated_at = now;
                    Ok(TaskMutationResult::Changed)
                },
            )
            .map_err(protocol_error_from_runtime)?;
        match result.outcome {
            TaskCommitOutcome::Committed(facts) => Ok(project_task_summary(facts.committed_task)),
            TaskCommitOutcome::Rejected(_) => {
                let current =
                    self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
                if matches!(current.lifecycle, TaskLifecycle::Archived) {
                    return Err(conflict_error(
                        "Archived Tasks are read-only; restore the Task before renaming it",
                    ));
                }
                Ok(project_task_summary(current))
            }
        }
    }
}

fn normalize_selection(selection: TaskTitleSelection) -> Result<TaskTitleSelection, ProtocolError> {
    let TaskTitleSelection::User { value } = selection else {
        return Ok(TaskTitleSelection::Automatic);
    };
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return Err(validation_error("title.value", "Task title is required"));
    }
    if value.chars().count() > MAX_USER_TITLE_CHARS {
        return Err(validation_error(
            "title.value",
            "Task title must be 200 characters or fewer",
        ));
    }
    Ok(TaskTitleSelection::User { value })
}
