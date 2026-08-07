use crate::protocol::errors::RuntimeError;
use crate::storage::task_journal::model::TaskOperation;

use super::RecoveredTask;

pub(super) fn remove(
    task: Option<&RecoveredTask>,
    operations: &mut Vec<TaskOperation>,
) -> Result<(), RuntimeError> {
    let current = match task {
        Some(RecoveredTask::Available { projection, .. }) => Some(projection.as_ref()),
        Some(RecoveredTask::Unavailable { error }) => {
            return Err(RuntimeError::Storage(error.clone()))
        }
        None => None,
    };
    operations.retain(|operation| match operation {
        TaskOperation::AppendText { text, .. } => !text.is_empty(),
        TaskOperation::ReplaceTask { task } => current
            .map(|projection| !serialized_equal(&projection.task, task.as_ref()))
            .unwrap_or(true),
        TaskOperation::ReplaceProjection { projection } => current
            .map(|current| !serialized_equal(current, projection.as_ref()))
            .unwrap_or(true),
        TaskOperation::ReplaceMessageMeta { message_meta } => current
            .map(|projection| !serialized_equal(&projection.message_meta, message_meta.as_ref()))
            .unwrap_or(true),
        TaskOperation::AppendMessage { .. }
        | TaskOperation::UpsertMessage { .. }
        | TaskOperation::ReplaceMessages { .. }
        | TaskOperation::Create { .. }
        | TaskOperation::CommitArtifact { .. } => true,
    });
    Ok(())
}

fn serialized_equal<T: serde::Serialize>(left: &T, right: &T) -> bool {
    serde_json::to_vec(left).ok() == serde_json::to_vec(right).ok()
}
