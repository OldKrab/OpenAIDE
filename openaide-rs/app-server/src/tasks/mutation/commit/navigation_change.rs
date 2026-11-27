use crate::storage::records::{TaskLifecycle, TaskRecord};
use openaide_app_server_protocol::ids::ProjectId;

/// The Project whose Navigation rows one durable Task mutation changed, if any. The Navigation
/// owner recomputes it; the commit only says where to look.
pub(super) fn navigation_change(
    original: &TaskRecord,
    task: &TaskRecord,
    summary_changed: bool,
    summary: &openaide_app_server_protocol::snapshot::TaskSummary,
) -> Option<ProjectId> {
    let original_visible = navigation_member(original);
    let task_visible = navigation_member(task);
    let changed = (original_visible || task_visible)
        && (original_visible != task_visible
            || original.lifecycle != task.lifecycle
            || original.last_activity != task.last_activity
            || original.tombstoned != task.tombstoned
            || summary_changed);
    changed.then(|| summary.project_id.clone())
}

pub(super) fn navigation_member(task: &TaskRecord) -> bool {
    matches!(
        task.lifecycle,
        TaskLifecycle::Open | TaskLifecycle::Archived
    ) && !task.tombstoned
}
