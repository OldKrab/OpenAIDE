use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{MessageId, TurnId};

use crate::protocol::errors::RuntimeError;
use crate::tasks::snapshot::build_snapshot;
use crate::tasks::transitions::TaskTransitions;

use super::{storage_error, TaskProductApi, TaskSendAccepted};

#[derive(Clone)]
pub(crate) struct CommittedSend {
    task_id: String,
    turn_id: TurnId,
    user_message_id: MessageId,
}

impl CommittedSend {
    pub(super) fn new(task_id: String, turn_id: TurnId, user_message_id: MessageId) -> Self {
        Self {
            task_id,
            turn_id,
            user_message_id,
        }
    }

    pub(super) fn fail(
        &self,
        api: &TaskProductApi,
        error: RuntimeError,
    ) -> Result<TaskSendAccepted, ProtocolError> {
        TaskTransitions::new(api.mutations.clone())
            .finish_turn(&self.task_id, self.turn_id.as_str(), Err(error))
            .map_err(super::super::protocol_error_from_runtime)?;
        self.accepted(api)
    }

    pub(super) fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub(super) fn accepted(&self, api: &TaskProductApi) -> Result<TaskSendAccepted, ProtocolError> {
        let snapshot = build_snapshot(&api.store, &self.task_id, 100).map_err(storage_error)?;
        Ok(TaskSendAccepted {
            task: api.project_task_snapshot(snapshot)?,
            turn_id: self.turn_id.clone(),
            user_message_id: self.user_message_id.clone(),
        })
    }
}
