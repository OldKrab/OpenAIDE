//! Keeps Task Chat storage free of image bytes (ADR-0075). The Task journal
//! store owns when this runs: before a frame is built, at hydration, and after
//! compaction.

use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use crate::protocol::errors::RuntimeError;
use crate::storage::chat_images;

use super::model::{TaskOperation, TaskProjection};

/// Writes inline images in a write's messages to the Task's image files and
/// leaves references, so neither the frame nor the projection holds bytes. Each
/// file is synced before the frame that references it is written.
pub(super) fn externalize_operations(
    task_dir: &Path,
    operations: &mut [TaskOperation],
) -> Result<usize, RuntimeError> {
    let mut moved = 0;
    for operation in operations {
        moved += match operation {
            TaskOperation::Create { projection }
            | TaskOperation::ReplaceProjection { projection } => {
                chat_images::externalize_messages(task_dir, &mut projection.messages)?
            }
            TaskOperation::AppendMessage { message } | TaskOperation::UpsertMessage { message } => {
                chat_images::externalize_message(task_dir, &mut message.chat.message)?
            }
            TaskOperation::ReplaceMessages { messages, .. } => {
                chat_images::externalize_messages(task_dir, messages)?
            }
            _ => 0,
        };
    }
    Ok(moved)
}

/// Moves a hydrated Chat's inline images into files. Returns false when there
/// were none. The caller then publishes a compacted generation; until it does,
/// the previous generation stays authoritative and the next hydration retries.
pub(super) fn externalize_projection(
    task_dir: &Path,
    projection: &mut TaskProjection,
) -> Result<bool, RuntimeError> {
    if !chat_images::has_inline_images(&projection.messages) {
        return Ok(false);
    }
    chat_images::externalize_messages(task_dir, &mut projection.messages)?;
    Ok(true)
}

/// Removes image files neither this Chat nor a Subagent history of the Task
/// references. A failure only leaves files for the next compaction.
pub(super) fn sweep_unreferenced(task_dir: &Path, projection: &TaskProjection) {
    let started_at = Instant::now();
    let mut referenced = HashSet::new();
    chat_images::referenced_ids(&projection.messages, &mut referenced);
    let result = chat_images::remove_unreferenced(task_dir, &referenced, |candidates| {
        crate::storage::subagents::subagent_image_references(task_dir, candidates)
    });
    match result {
        Ok(0) => {}
        Ok(removed) => crate::logging::info(
            "chat_images_swept",
            serde_json::json!({
                "task_id": projection.task.task_id,
                "removed_count": removed,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        ),
        Err(error) => crate::logging::warn(
            "chat_images_sweep_failed",
            serde_json::json!({
                "task_id": projection.task.task_id,
                "error_kind": error.reason(),
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        ),
    }
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
