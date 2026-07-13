use openaide_app_server_protocol::ids::ClientInstanceId;

use crate::protocol::errors::RuntimeError;
use crate::storage::records::{TaskLifecycle, TaskRecord};

/// Enforces client-private New Task ownership without revealing another client's Task.
pub(crate) fn require_client_task_access(
    task: &TaskRecord,
    client_instance_id: &ClientInstanceId,
) -> Result<(), RuntimeError> {
    match &task.lifecycle {
        TaskLifecycle::Visible => Ok(()),
        TaskLifecycle::New {
            owner_client_instance_id,
        } if owner_client_instance_id == client_instance_id => Ok(()),
        TaskLifecycle::New { .. } => Err(RuntimeError::TaskNotFound(task.task_id.clone())),
    }
}
