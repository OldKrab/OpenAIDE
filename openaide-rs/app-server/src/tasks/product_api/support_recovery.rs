use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::support::{
    SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult,
};

use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::storage::records::TaskRecord;
use crate::tasks::snapshot::build_snapshot;
use crate::tasks::transitions::{ActiveWorkEnd, TaskTransitions};

use super::{protocol_error_from_runtime, storage_error, TaskProductApi};

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
        let ended = TaskTransitions::new(self.mutations.clone(), self.server_requests.clone())
            .end_active_work(
                &candidate.task_id,
                candidate.active_turn_id.as_deref(),
                ActiveWorkEnd::SupportRecovery,
            )
            .map_err(protocol_error_from_runtime)?;
        if !ended {
            return Ok(None);
        }
        if let Some(turn_id) = candidate.active_turn_id.as_deref() {
            self.turn_runner.detach_stuck_turn(turn_id);
            self.turn_acceptance
                .retire_pending_turn(&candidate.task_id, turn_id);
        }
        let snapshot =
            build_snapshot(&self.store, &candidate.task_id, 100).map_err(storage_error)?;
        self.project_task_snapshot(snapshot).map(Some)
    }
}
