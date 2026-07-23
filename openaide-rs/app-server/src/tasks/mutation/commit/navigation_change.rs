use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::task_events::CommittedNavigationChange;

/// Projects one durable Task mutation onto the smallest safe Navigation event.
pub(super) fn navigation_change(
    original: &TaskRecord,
    task: &TaskRecord,
    summary_changed: bool,
    summary: &openaide_app_server_protocol::snapshot::TaskSummary,
) -> Option<CommittedNavigationChange> {
    let original_visible = navigation_member(original);
    let task_visible = navigation_member(task);
    let membership_or_order_changed = original_visible != task_visible
        || original.lifecycle != task.lifecycle
        || original.last_activity != task.last_activity
        || original.tombstoned != task.tombstoned;
    match (original_visible, task_visible) {
        (_, true) if membership_or_order_changed => {
            Some(CommittedNavigationChange::ProjectEntriesChanged {
                project_id: summary.project_id.clone(),
            })
        }
        (true, false) => Some(CommittedNavigationChange::ProjectEntriesChanged {
            project_id: summary.project_id.clone(),
        }),
        (true, true) if summary_changed => Some(CommittedNavigationChange::TaskUpdated(Box::new(
            summary.clone(),
        ))),
        _ => None,
    }
}

pub(super) fn navigation_member(task: &TaskRecord) -> bool {
    matches!(
        task.lifecycle,
        TaskLifecycle::Open | TaskLifecycle::Archived
    ) && !task.tombstoned
}
