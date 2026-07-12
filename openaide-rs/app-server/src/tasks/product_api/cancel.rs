use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::task::TaskCancelParams;
use uuid::Uuid;

use crate::protocol::model::{
    ActivityStatus, InterruptionReason, NormalizedMessage, TaskStatus as LegacyTaskStatus,
};
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::tasks::snapshot::build_snapshot;
use crate::time::now_string;

use super::{
    conflict_error, internal_error, protocol_error_from_runtime, runtime_error, storage_error,
    TaskProductApi,
};

impl TaskProductApi {
    pub(super) fn cancel_task(
        &self,
        params: TaskCancelParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.turn_acceptance
            .serialize(&task_id, || self.cancel_task_serialized(params))
    }

    fn cancel_task_serialized(
        &self,
        params: TaskCancelParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let task = self.store.read_task(&task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&task)?;
        let Some(active_turn_id) = task.active_turn_id.clone() else {
            let snapshot = build_snapshot(&self.store, &task_id, 100).map_err(storage_error)?;
            return self.project_task_snapshot(snapshot);
        };
        if let Some(expected) = params.turn_id.as_ref() {
            if expected.as_str() != active_turn_id {
                return Err(conflict_error("Task turn is not active"));
            }
        }
        let now = now_string();
        let result = self
            .mutations
            .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                if ctx.task().tombstoned {
                    return Ok(TaskMutationResult::Rejected);
                }
                if ctx.task().active_turn_id.as_deref() != Some(active_turn_id.as_str()) {
                    return Ok(TaskMutationResult::Rejected);
                }
                ctx.finish_running_activities(ActivityStatus::Completed)?;
                ctx.cancel_pending_permissions()?;
                ctx.cancel_pending_questions()?;
                ctx.append_message(NormalizedMessage::Interruption {
                    id: Uuid::new_v4().to_string(),
                    reason: InterruptionReason::Canceled,
                    message: "Task was stopped.".to_string(),
                    created_at: now.clone(),
                    recoverable: true,
                })?;

                let task = ctx.task_mut();
                task.status = LegacyTaskStatus::Inactive;
                task.active_turn_id = None;
                task.updated_at = now.clone();
                task.last_activity = now;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(conflict_error("Task turn is not active"));
        }
        // Keep Task acceptance serialized through generation retirement. Failed persistence
        // leaves the accepted Turn untouched, and a successful cancel cannot invalidate a newer
        // send between its exact-Turn commit and startup cancellation.
        let cancellation_generation = self.history_sync.begin_send(&task_id);
        self.turn_runner.cancel_turn(&active_turn_id);
        self.turn_acceptance
            .retire_pending_turn(&task_id, &active_turn_id);
        self.pending_send_sync.take(&task_id);
        self.publish_history_sync(
            &task_id,
            openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle {
                generation: cancellation_generation,
            },
        );
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing task cancel snapshot"))?;
        self.project_task_snapshot(snapshot)
    }
}
