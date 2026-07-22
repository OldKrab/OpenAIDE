use crate::protocol::errors::RuntimeError;
use crate::storage::records::TaskRecord;
use crate::task_recovery::volatile_recovery_plan;
use crate::tasks::mutation::TaskMutationResult;
use crate::time::now_string;

use super::active_work_end::apply_active_work_end;
use super::helpers::chat_commit_options;
use super::{ActiveWorkEnd, TaskTransitions};

impl TaskTransitions {
    pub(crate) fn recover_volatile_runtime_state(&self) -> Result<(), RuntimeError> {
        for task in self.recoverable_records()? {
            // Catalog metadata is sufficient to exclude durable idle Tasks.
            // Avoid replaying their Chat during process startup; the mutation
            // closure rechecks the plan after loading Tasks that need repair.
            if volatile_recovery_plan(&task).is_none() {
                continue;
            }
            let task_id = task.task_id;
            let mut active_work_ended = false;
            self.mutations
                .commit_existing_task(&task_id, chat_commit_options(), |ctx| {
                    let now = now_string();
                    let Some(plan) = volatile_recovery_plan(ctx.task()) else {
                        return Ok(TaskMutationResult::Unchanged);
                    };
                    if plan.interrupt_active_turn {
                        apply_active_work_end(ctx, &ActiveWorkEnd::Restarted, now.clone())?;
                        active_work_ended = true;
                    }
                    if plan.invalidate_live_session_data {
                        let task = ctx.task_mut();
                        task.config_options_catalog = None;
                        task.agent_commands_catalog = None;
                    }
                    if plan.clear_pending_config_change {
                        // Agent I/O cannot be resumed after process restart. Preserve the
                        // monotonic sequence, but retire the volatile client mutation so
                        // App Shells can issue a fresh, explicitly ordered request.
                        ctx.task_mut().config_mutation.pending = None;
                    }

                    Ok(TaskMutationResult::Changed)
                })?;
            if active_work_ended {
                self.close_task_requests(&task_id);
            }
        }

        Ok(())
    }

    fn recoverable_records(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        let _guard = self.mutations.lock();
        self.mutations.store().list_all_task_records()
    }
}
