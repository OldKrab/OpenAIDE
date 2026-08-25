use crate::protocol::errors::RuntimeError;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutations};

pub(in crate::tasks::mutation) fn purge_task_if_retention_expired(
    target: &TaskMutations,
    task_id: &str,
    now_millis: i128,
    cutoff_millis: i128,
    process_protected: bool,
    before_purge: impl FnOnce(&crate::storage::records::TaskRecord) -> Result<(), RuntimeError>,
) -> Result<bool, RuntimeError> {
    let guard = target.lock();
    let mut task = target.store.read_task(task_id)?;
    if task.tombstoned {
        drop(guard);
        target.store.task_journal().purge_tombstoned_task(task_id)?;
        return Ok(true);
    }
    let marker = target.store.task_journal().task_last_used_millis(task_id)?;
    if marker.is_none() {
        // Older stores could not record client opens. Start a measured grace
        // period instead of destructively guessing at legacy read recency.
        target
            .store
            .task_journal()
            .mark_task_used(task_id, now_millis)?;
        return Ok(false);
    }
    if !crate::tasks::retention::is_expired_idle_unpinned(
        &task,
        marker,
        cutoff_millis,
        process_protected,
    ) {
        return Ok(false);
    }

    // Persist any cross-store visibility intent before the Task tombstone. A crash
    // must not delete the Task first and let its Agent session reappear in Navigation.
    before_purge(&task)?;

    let original = task.clone();
    task.tombstoned = true;
    let facts = super::persist_changed_task(
        target,
        &original,
        &mut task,
        TaskCommitOptions::metadata(),
        Vec::new(),
        Vec::new(),
    )?;
    super::notify_task_changed(target, &facts);
    drop(guard);
    target.store.task_journal().purge_tombstoned_task(task_id)?;
    Ok(true)
}
