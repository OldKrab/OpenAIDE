use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{InterruptionReason, TaskStatus};
use crate::tasks::mutation::TaskMutationResult;
use crate::time::now_string;

use super::helpers::{append_interruption, chat_commit_options};
use super::TaskTransitions;

impl TaskTransitions {
    pub(crate) fn fail_created_task_start(
        &self,
        task_id: &str,
        error: &RuntimeError,
    ) -> Result<(), RuntimeError> {
        self.end_active_work(
            task_id,
            None,
            super::ActiveWorkEnd::AgentStartFailed(error.to_string()),
        )?;
        Ok(())
    }

    pub(crate) fn fail_adopted_task_attach(
        &self,
        task_id: &str,
        session_id: &str,
        error: &RuntimeError,
    ) -> Result<(), RuntimeError> {
        let result = self
            .mutations
            .commit_existing_task(task_id, chat_commit_options(), |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(session_id) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let now = now_string();
                append_interruption(
                    ctx,
                    InterruptionReason::Failed,
                    &error.to_string(),
                    now.clone(),
                    true,
                )?;

                let task = ctx.task_mut();
                task.status = TaskStatus::Failed;
                task.unread = true;
                task.updated_at = now.clone();
                task.last_activity = now;
                Ok(TaskMutationResult::Changed)
            });

        match result {
            Ok(_) | Err(RuntimeError::TaskNotFound(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }
}
