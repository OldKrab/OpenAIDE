use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, InterruptionReason, TaskStatus};
use crate::storage::records::TaskRecord;
use crate::task_recovery::{volatile_recovery_plan, RESTART_INTERRUPTION_MESSAGE};
use crate::tasks::mutation::TaskMutationResult;
use crate::time::now_string;

use super::helpers::{append_interruption, chat_commit_options};
use super::TaskTransitions;

impl TaskTransitions {
    pub(crate) fn recover_volatile_runtime_state(&self) -> Result<(), RuntimeError> {
        for task in self.recoverable_records()? {
            let task_id = task.task_id;
            self.mutations
                .commit_existing_task(&task_id, chat_commit_options(), |ctx| {
                    let now = now_string();
                    let questions_cancelled = ctx.cancel_pending_questions()?;
                    let Some(plan) = volatile_recovery_plan(ctx.task()) else {
                        if questions_cancelled && ctx.task().status == TaskStatus::Blocked {
                            ctx.task_mut().status = TaskStatus::Inactive;
                            ctx.task_mut().updated_at = now;
                            return Ok(TaskMutationResult::Changed);
                        }
                        return Ok(TaskMutationResult::Unchanged);
                    };
                    if plan.interrupt_active_turn {
                        ctx.finish_running_activities(ActivityStatus::Completed)?;
                        ctx.cancel_pending_permissions()?;
                        append_interruption(
                            ctx,
                            InterruptionReason::Canceled,
                            RESTART_INTERRUPTION_MESSAGE,
                            now.clone(),
                            true,
                        )?;
                        let task = ctx.task_mut();
                        task.status = TaskStatus::Inactive;
                        task.active_turn_id = None;
                        task.unread = true;
                        task.last_activity = now.clone();
                    }

                    Ok(TaskMutationResult::Changed)
                })?;
        }

        Ok(())
    }

    fn recoverable_records(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        let _guard = self.mutations.lock();
        let mut records = self.mutations.store().list_tasks()?;
        records.extend(self.mutations.store().list_archived_tasks()?);
        Ok(records)
    }
}
