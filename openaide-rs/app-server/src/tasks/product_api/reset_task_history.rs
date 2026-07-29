use crate::agent::AgentSessionKey;
use crate::protocol_edge::RemovedTask;
use openaide_app_server_protocol::errors::ProtocolError;

use super::{protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    /// Settles runtime ownership before removing only OpenAIDE's local Task data.
    pub(super) fn reset_local_task_history(&self) -> Result<Vec<RemovedTask>, ProtocolError> {
        let _adoption = self
            .native_adoption
            .lock()
            .expect("native adoption lock poisoned");
        self.turn_runner
            .settle_for_task_history_reset()
            .map_err(protocol_error_from_runtime)?;

        let tasks = self.store.task_journal().list_task_records();
        let removed_tasks = tasks
            .iter()
            .map(|task| RemovedTask {
                task_id: task.task_id.clone().into(),
                next_revision: task.revision.saturating_add(1),
            })
            .collect::<Vec<_>>();
        for task in &tasks {
            let Some(session_id) = task.agent_session_id.as_ref() else {
                continue;
            };
            self.turn_runner
                .close_session_for_task_history_reset(&AgentSessionKey::new(
                    task.agent_id.clone(),
                    session_id.clone(),
                ))
                .map_err(protocol_error_from_runtime)?;
        }

        self.store
            .reset_task_history()
            .map_err(protocol_error_from_runtime)?;
        self.turn_acceptance.clear();
        self.history_sync.clear_task_state();
        crate::logging::info(
            "task_history_reset_completed",
            serde_json::json!({ "task_count": removed_tasks.len() }),
        );
        Ok(removed_tasks)
    }
}
