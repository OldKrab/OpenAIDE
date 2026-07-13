use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::TaskCancelParams;

use crate::agent::AgentPromptOutcome;
use crate::tasks::snapshot::build_snapshot;
use crate::tasks::transitions::TaskTransitions;

use super::{
    conflict_error, protocol_error_from_runtime, runtime_error, storage_error, TaskProductApi,
};

impl TaskProductApi {
    pub(super) fn cancel_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskCancelParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.read_task_for_client(&task_id, client_instance_id)?;
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
        let transitions =
            TaskTransitions::new(self.mutations.clone(), self.server_requests.clone());
        if !transitions
            .mark_turn_stopping(&task_id, &active_turn_id)
            .map_err(protocol_error_from_runtime)?
        {
            return Err(conflict_error("Task turn is not active"));
        }

        // Stop closes Task-scoped requests before ACP cancellation. Task completion remains
        // owned by the primary prompt response, except when no prompt was started.
        match self.turn_runner.cancel_turn(&active_turn_id) {
            Ok(true) => {}
            Ok(false) => {
                transitions
                    .finish_turn(&task_id, &active_turn_id, Ok(AgentPromptOutcome::Cancelled))
                    .map_err(protocol_error_from_runtime)?;
            }
            Err(error) => {
                transitions
                    .finish_turn(&task_id, &active_turn_id, Err(error))
                    .map_err(protocol_error_from_runtime)?;
            }
        }
        self.turn_acceptance
            .retire_pending_turn(&task_id, &active_turn_id);
        let snapshot = build_snapshot(&self.store, &task_id, 100).map_err(storage_error)?;
        self.project_task_snapshot(snapshot)
    }
}
