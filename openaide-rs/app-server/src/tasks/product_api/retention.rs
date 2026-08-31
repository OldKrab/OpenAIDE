use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::protocol::errors::RuntimeError;
use crate::tasks::retention::{
    storage_is_idle, TaskStorageMaintenanceReport, DEFAULT_TASK_RETENTION_MILLIS,
};

use super::TaskProductApi;

#[derive(Clone, Default)]
pub(super) struct TaskStorageMaintenanceCoordinator {
    running: Arc<AtomicBool>,
}

impl TaskProductApi {
    /// Coalesces timer/startup requests so only one root scan can run at once.
    pub(crate) fn request_task_storage_maintenance(&self) {
        if self
            .storage_maintenance
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let api = self.clone();
        std::thread::spawn(move || {
            let now = crate::time::activity_millis(&crate::time::now_string());
            if let Some(now) = now {
                if let Err(error) = api.run_storage_maintenance_at(now) {
                    crate::logging::warn(
                        "task_storage_maintenance_failed",
                        serde_json::json!({ "error": error.to_string() }),
                    );
                }
            }
            api.storage_maintenance
                .running
                .store(false, Ordering::Release);
        });
    }

    /// Runs one bounded pass. Native Sessions and Worktrees are deliberately
    /// outside this local Task-store maintenance boundary.
    pub(crate) fn run_storage_maintenance_at(
        &self,
        now_millis: i128,
    ) -> Result<TaskStorageMaintenanceReport, RuntimeError> {
        let cutoff_millis = now_millis.saturating_sub(DEFAULT_TASK_RETENTION_MILLIS);
        let mut report = TaskStorageMaintenanceReport::default();
        for task in self.store.list_all_task_records()? {
            if task.tombstoned {
                match self.mutations.purge_existing_tombstone(&task.task_id) {
                    Ok(()) => report.purged_tasks += 1,
                    Err(error) => {
                        record_failure(&task.task_id, "purge_tombstone", error, &mut report)
                    }
                }
                continue;
            }
            let process_protected = self
                .task_subscription_presence
                .has_subscribers(&task.task_id)
                || self.server_requests.has_pending_for_task(
                    &openaide_app_server_protocol::ids::TaskId::from(task.task_id.clone()),
                );
            let purged = match self.mutations.purge_task_if_retention_expired(
                &task.task_id,
                now_millis,
                cutoff_millis,
                process_protected,
                |expiring| {
                    let Some(session_id) = expiring.agent_session_id.as_deref() else {
                        return Ok(());
                    };
                    self.native_catalog.archive_task_identity(
                        &crate::native_sessions::catalog::NativeSessionRef::new(
                            &expiring.agent_id,
                            session_id,
                        ),
                    )
                },
            ) {
                Ok(true) => {
                    report.purged_tasks += 1;
                    true
                }
                Ok(false) => false,
                Err(error) => {
                    record_failure(&task.task_id, "retention", error, &mut report);
                    false
                }
            };
            if !purged && !process_protected && storage_is_idle(&task) {
                report.compacted_tasks_checked += 1;
                if let Err(error) = self.mutations.maintain_task_storage(&task.task_id) {
                    record_failure(&task.task_id, "compact", error, &mut report);
                }
            }
        }
        crate::logging::info(
            "task_storage_maintenance_completed",
            serde_json::json!({
                "compacted_tasks_checked": report.compacted_tasks_checked,
                "purged_tasks": report.purged_tasks,
                "failed_tasks": report.failed_tasks,
            }),
        );
        Ok(report)
    }

    pub(super) fn mark_task_used_now(&self, task_id: &str) -> Result<(), RuntimeError> {
        let now = crate::time::activity_millis(&crate::time::now_string()).ok_or_else(|| {
            RuntimeError::Internal("current Task usage time is invalid".to_string())
        })?;
        self.mutations.mark_task_used(task_id, now)
    }

    #[cfg(test)]
    pub(crate) fn mark_task_used_at_for_test(
        &self,
        task_id: &str,
        used_at_millis: i128,
    ) -> Result<(), RuntimeError> {
        self.mutations.mark_task_used(task_id, used_at_millis)
    }
}

impl super::TaskStorageMaintenanceWorkflow for TaskProductApi {
    fn request_task_storage_maintenance(&self) {
        TaskProductApi::request_task_storage_maintenance(self)
    }
}

fn record_failure(
    task_id: &str,
    stage: &'static str,
    error: RuntimeError,
    report: &mut TaskStorageMaintenanceReport,
) {
    report.failed_tasks += 1;
    crate::logging::warn(
        "task_storage_maintenance_task_failed",
        serde_json::json!({
            "task_id": task_id,
            "stage": stage,
            "error": error.to_string(),
        }),
    );
}
