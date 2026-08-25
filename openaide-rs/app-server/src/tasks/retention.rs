use crate::protocol::model::TaskStatus;
use crate::storage::records::{TaskLifecycle, TaskRecord};

pub(crate) const DEFAULT_TASK_RETENTION_MILLIS: i128 = 7 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TaskStorageMaintenanceReport {
    pub(crate) compacted_tasks_checked: usize,
    pub(crate) purged_tasks: usize,
    pub(crate) failed_tasks: usize,
}

pub(super) fn is_expired_idle_unpinned(
    task: &TaskRecord,
    usage_marker_millis: Option<i128>,
    cutoff_millis: i128,
    process_protected: bool,
) -> bool {
    if process_protected
        || task.tombstoned
        || task.pinned
        || matches!(task.lifecycle, TaskLifecycle::Prepared { .. })
        || !storage_is_idle(task)
        || !task.message_queue.items.is_empty()
    {
        return false;
    }
    [
        crate::time::activity_millis(&task.last_activity),
        usage_marker_millis,
    ]
    .into_iter()
    .flatten()
    .max()
    .is_some_and(|last_used| last_used <= cutoff_millis)
}

pub(super) fn storage_is_idle(task: &TaskRecord) -> bool {
    task.active_turn_id.is_none()
        && !matches!(
            task.status,
            TaskStatus::Starting | TaskStatus::Active | TaskStatus::Stopping | TaskStatus::Waiting
        )
}
