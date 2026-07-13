use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::support::{
    SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult,
};
use uuid::Uuid;

use crate::protocol::model::{
    ActivityStatus, InterruptionReason, NormalizedMessage, TaskStatus as LegacyTaskStatus,
};
use crate::storage::records::TaskRecord;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::{protocol_error_from_runtime, storage_error, TaskProductApi};

const SUPPORT_RECOVERY_INTERRUPTION_MESSAGE: &str =
    "Task was stopped by support recovery because the session appeared stuck.";

impl TaskProductApi {
    pub(super) fn recover_stuck_sessions(
        &self,
        _params: SupportRecoverStuckSessionsParams,
    ) -> Result<SupportRecoverStuckSessionsResult, ProtocolError> {
        let candidates = self.stuck_session_candidates().map_err(storage_error)?;
        let mut recovered_tasks = Vec::new();
        for candidate in candidates {
            if let Some(task) = self.recover_stuck_session_candidate(candidate)? {
                recovered_tasks.push(task);
            }
        }
        Ok(SupportRecoverStuckSessionsResult { recovered_tasks })
    }

    fn stuck_session_candidates(
        &self,
    ) -> Result<Vec<TaskRecord>, crate::protocol::errors::RuntimeError> {
        let _guard = self.mutations.lock();
        Ok(self
            .store
            .list_all_task_records()?
            .into_iter()
            .filter(|task| !task.tombstoned)
            .filter(|task| task.status == LegacyTaskStatus::Active || task.active_turn_id.is_some())
            .collect())
    }

    fn recover_stuck_session_candidate(
        &self,
        candidate: TaskRecord,
    ) -> Result<Option<TaskSnapshot>, ProtocolError> {
        let task_id = candidate.task_id.clone();
        self.turn_acceptance.serialize(&task_id, || {
            self.recover_stuck_session_candidate_serialized(candidate)
        })
    }

    fn recover_stuck_session_candidate_serialized(
        &self,
        candidate: TaskRecord,
    ) -> Result<Option<TaskSnapshot>, ProtocolError> {
        let result = self
            .mutations
            .commit_existing_task(
                &candidate.task_id,
                super::response_snapshot_options(),
                |ctx| {
                    if ctx.task().active_turn_id != candidate.active_turn_id
                        || (candidate.active_turn_id.is_none()
                            && ctx.task().status != LegacyTaskStatus::Active)
                    {
                        return Ok(TaskMutationResult::Unchanged);
                    }

                    let now = now_string();
                    ctx.finish_running_activities(ActivityStatus::Completed)?;
                    ctx.append_message(NormalizedMessage::Interruption {
                        id: Uuid::new_v4().to_string(),
                        reason: InterruptionReason::Canceled,
                        message: SUPPORT_RECOVERY_INTERRUPTION_MESSAGE.to_string(),
                        created_at: now.clone(),
                        recoverable: true,
                    })?;

                    let task = ctx.task_mut();
                    task.status = LegacyTaskStatus::Inactive;
                    task.active_turn_id = None;
                    task.unread = true;
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                },
            )
            .map_err(protocol_error_from_runtime)?;

        match result.outcome {
            TaskCommitOutcome::Committed(_) => {
                if let Some(turn_id) = candidate.active_turn_id.as_deref() {
                    self.turn_runner.detach_stuck_turn(turn_id);
                    self.turn_acceptance
                        .retire_pending_turn(&candidate.task_id, turn_id);
                }
                let snapshot = result
                    .response_snapshot
                    .ok_or_else(|| super::internal_error("missing support recovery snapshot"))?;
                self.project_task_snapshot(snapshot).map(Some)
            }
            TaskCommitOutcome::Rejected(_) => Ok(None),
        }
    }
}
