use openaide_app_server_protocol::ids::ClientInstanceId;

use crate::protocol::errors::RuntimeError;
use crate::storage::records::{TaskLifecycle, TaskRecord};

/// Enforces an exclusive Prepared-Task lease without revealing another client's Task.
pub(crate) fn require_client_task_access(
    task: &TaskRecord,
    client_instance_id: &ClientInstanceId,
) -> Result<(), RuntimeError> {
    match &task.lifecycle {
        TaskLifecycle::Open | TaskLifecycle::Archived => Ok(()),
        TaskLifecycle::Prepared {
            lease: Some(lessee),
        } if lessee == client_instance_id => Ok(()),
        TaskLifecycle::Prepared { .. } => Err(RuntimeError::TaskNotFound(task.task_id.clone())),
    }
}
