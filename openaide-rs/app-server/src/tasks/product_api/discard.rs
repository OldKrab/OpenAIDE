use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskNavigationSnapshot;
use openaide_app_server_protocol::task::TaskDiscardParams;

use crate::agent::AgentSessionKey;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::snapshots::{TaskNavigationSnapshotSource, TaskNavigationStore};
use crate::storage::records::TaskRecord;
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::{conflict_error, protocol_error_from_runtime, runtime_error, TaskProductApi};

impl TaskProductApi {
    pub(super) fn discard_task(
        &self,
        params: TaskDiscardParams,
    ) -> Result<TaskNavigationSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let task = self.store.read_task(&task_id).map_err(runtime_error)?;
        self.require_discard_eligible(&task)?;

        if task.tombstoned {
            return self.task_navigation();
        }
        let now = now_string();
        let mut session_to_close = None;
        let result = self
            .mutations
            .commit_existing_task(&task_id, TaskCommitOptions::metadata(), |ctx| {
                if !self.is_discard_eligible(ctx.task())? {
                    return Ok(TaskMutationResult::Rejected);
                }
                let task = ctx.task_mut();
                session_to_close = task.agent_session_id.take();
                task.tombstoned = true;
                task.updated_at = now.clone();
                task.last_activity = now;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(conflict_error("Only empty pre-send Tasks can be discarded"));
        }
        // A discarded Draft Task cannot have a live composer again. Remove every
        // resolver owned by that Task, including resources from disconnected clients.
        self.attachments.discard_resources_for_task(&params.task_id);
        // Persist ownership release before Agent I/O so a failed close cannot
        // leave the discarded Draft Task hiding the external Native Session.
        if let Some(session_id) = session_to_close {
            let session = AgentSessionKey::new(task.agent_id, session_id.clone());
            if let Err(error) = self.agent_gateway.close_session(&session) {
                crate::logging::warn(
                    "discarded_draft_native_session_close_failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "agent_session_id": session_id,
                        "error": error.to_string(),
                    }),
                );
            }
        }
        self.task_navigation()
    }

    fn task_navigation(&self) -> Result<TaskNavigationSnapshot, ProtocolError> {
        TaskNavigationStore::new(self.store.clone()).snapshot(None)
    }

    fn require_discard_eligible(&self, task: &TaskRecord) -> Result<(), ProtocolError> {
        if self.is_discard_eligible(task).map_err(runtime_error)? {
            return Ok(());
        }
        Err(conflict_error(discard_ineligible_message(task)))
    }

    fn is_discard_eligible(&self, task: &TaskRecord) -> Result<bool, RuntimeError> {
        if task.status == LegacyTaskStatus::Active || task.active_turn_id.is_some() {
            return Ok(false);
        }
        Ok(!task.first_prompt_sent && self.store.read_messages(&task.task_id)?.is_empty())
    }
}

fn discard_ineligible_message(task: &TaskRecord) -> &'static str {
    if task.status == LegacyTaskStatus::Active || task.active_turn_id.is_some() {
        "Running Tasks cannot be discarded"
    } else {
        "Only empty pre-send Tasks can be discarded"
    }
}
