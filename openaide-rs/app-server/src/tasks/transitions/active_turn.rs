use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, InterruptionReason, TaskStatus};
use crate::storage::records::TaskRecord;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::helpers::{append_interruption, chat_commit_options};
use super::TaskTransitions;

impl TaskTransitions {
    pub(crate) fn active_turn_id(&self, task_id: &str) -> Result<Option<String>, RuntimeError> {
        let _guard = self.mutations.lock();
        Ok(self.mutations.store().read_task(task_id)?.active_turn_id)
    }

    pub(crate) fn cancel_running_task(
        &self,
        task_id: &str,
        expected_turn_id: Option<&str>,
        message: &str,
        unread: bool,
    ) -> Result<bool, RuntimeError> {
        let result =
            self.mutations
                .commit_existing_task(task_id, chat_commit_options(), |ctx| {
                    if !active_turn_matches(ctx.task(), expected_turn_id) {
                        return Ok(TaskMutationResult::Unchanged);
                    }

                    let now = now_string();
                    ctx.finish_running_activities(ActivityStatus::Completed)?;
                    ctx.cancel_pending_permissions()?;
                    ctx.cancel_pending_questions()?;
                    append_interruption(
                        ctx,
                        InterruptionReason::Canceled,
                        message,
                        now.clone(),
                        true,
                    )?;

                    let task = ctx.task_mut();
                    task.status = TaskStatus::Inactive;
                    task.active_turn_id = None;
                    task.unread |= unread;
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                })?;
        Ok(matches!(result.outcome, TaskCommitOutcome::Committed(_)))
    }

    pub(crate) fn finish_turn(
        &self,
        task_id: &str,
        turn_id: &str,
        result: Result<(), RuntimeError>,
    ) -> Result<bool, RuntimeError> {
        let commit =
            self.mutations
                .commit_existing_task(task_id, chat_commit_options(), |ctx| {
                    if ctx.task().active_turn_id.as_deref() != Some(turn_id) {
                        return Ok(TaskMutationResult::Unchanged);
                    }

                    let now = now_string();
                    match &result {
                        Ok(()) => {
                            ctx.finish_running_activities(ActivityStatus::Completed)?;
                            ctx.task_mut().status = TaskStatus::Inactive;
                        }
                        Err(error) => {
                            ctx.finish_running_activities(ActivityStatus::Error)?;
                            append_interruption(
                                ctx,
                                InterruptionReason::Failed,
                                &error.to_string(),
                                now.clone(),
                                true,
                            )?;
                            ctx.task_mut().status = TaskStatus::Failed;
                        }
                    }

                    let task = ctx.task_mut();
                    task.active_turn_id = None;
                    task.unread = true;
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                })?;
        Ok(matches!(commit.outcome, TaskCommitOutcome::Committed(_)))
    }
}

fn active_turn_matches(task: &TaskRecord, expected_turn_id: Option<&str>) -> bool {
    match (task.active_turn_id.as_deref(), expected_turn_id) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(active), Some(expected)) => active == expected,
    }
}
