//! Serves one stored Chat image of a Task by id (ADR-0075).

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};

use crate::storage::Store;

/// Checks Task access exactly like Tool image previews, then reads only from
/// that Task's images, so an id from another Task is not found.
pub(super) fn load(
    store: &Store,
    client_instance_id: &ClientInstanceId,
    task_id: &TaskId,
    image_id: &str,
) -> Result<Vec<u8>, ProtocolError> {
    let task = store
        .read_task(task_id.as_str())
        .map_err(super::task_snapshot_error)?;
    crate::tasks::access::require_client_task_access(&task, client_instance_id)
        .map_err(super::task_snapshot_error)?;
    store
        .read_chat_image(task_id.as_str(), image_id)
        .map_err(super::task_snapshot_error)?
        .ok_or_else(not_found)
}

pub(super) fn not_found() -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotFound,
        message: "image not found".to_string(),
        recoverable: false,
        target: None,
    }
}
