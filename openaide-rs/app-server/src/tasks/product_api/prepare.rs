use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::storage::records::{TaskLifecycle, TaskPreparationRecord, TaskRecord};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};
use crate::time::now_string;

use super::{internal_error, TaskProductApi};

impl TaskProductApi {
    pub(super) fn spawn_task_preparation(&self, task: TaskRecord) {
        let api = self.clone();
        std::thread::spawn(move || {
            if let Err(error) = api.native_sessions.prepare_task(&task) {
                let _ = api.persist_preparation_failure(&task.task_id, &error);
            }
            match api.mutations.reconcile_prepared_task_pool(false) {
                Ok(disposed) => api.close_disposed_prepared_tasks(disposed),
                Err(error) => crate::logging::error(
                    "prepared_task_pool_reconcile_failed",
                    serde_json::json!({ "task_id": task.task_id, "error": error.to_string() }),
                ),
            }
        });
    }

    fn persist_preparation_failure(
        &self,
        task_id: &str,
        error: &RuntimeError,
    ) -> Result<(), RuntimeError> {
        let message = error.to_string();
        let now = now_string();
        self.mutations.commit_existing_task(
            task_id,
            TaskCommitOptions::metadata(),
            move |ctx| {
                if ctx.task().tombstoned
                    || !matches!(ctx.task().preparation, TaskPreparationRecord::Preparing)
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                // Catalogs are live Native Session data. If attachment or finalization
                // failed, no closed session may remain the source of visible controls.
                task.config_options_catalog = None;
                task.agent_commands_catalog = None;
                task.preparation = TaskPreparationRecord::Failed { message };
                task.updated_at = now;
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    pub(super) fn recover_abandoned_preparations(&self) -> Result<(), RuntimeError> {
        for task in self.store.list_all_task_records()? {
            if !is_abandoned_preparation(&task) {
                continue;
            }
            let message = "Task Agent preparation was interrupted before it finished".to_string();
            self.mutations.commit_existing_task(
                &task.task_id,
                TaskCommitOptions::metadata(),
                move |ctx| {
                    if !is_abandoned_preparation(ctx.task()) {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    let task = ctx.task_mut();
                    // A crash may happen after binding but before sink attachment or readiness.
                    // Preserve the durable Native Session identity so retry can resume it.
                    task.config_options_catalog = None;
                    task.agent_commands_catalog = None;
                    task.preparation = TaskPreparationRecord::Failed { message };
                    Ok(TaskMutationResult::Changed)
                },
            )?;
        }
        Ok(())
    }
}

pub(super) fn reject_if_preparation_not_ready(task: &TaskRecord) -> Result<(), ProtocolError> {
    match &task.preparation {
        TaskPreparationRecord::Ready => Ok(()),
        TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing => Err(ProtocolError {
            code: ProtocolErrorCode::Conflict,
            message: "Task Agent preparation is still running".to_string(),
            recoverable: true,
            target: None,
        }),
        TaskPreparationRecord::Failed { message } => Err(ProtocolError {
            code: ProtocolErrorCode::Internal,
            message: format!("Task Agent preparation failed: {message}"),
            recoverable: true,
            target: None,
        }),
    }
}

fn is_abandoned_preparation(task: &TaskRecord) -> bool {
    !task.tombstoned
        && matches!(task.lifecycle, TaskLifecycle::New { .. })
        && task.status == LegacyTaskStatus::Inactive
        && matches!(
            task.preparation,
            TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing
        )
}

pub(super) fn missing_prepared_task_snapshot() -> ProtocolError {
    internal_error("missing task preparation snapshot")
}
