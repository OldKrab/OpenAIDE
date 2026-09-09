use super::*;
use crate::storage::records::TaskLifecycle;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use openaide_app_server_protocol::snapshot::AgentConfigPreferencesState;
use openaide_app_server_protocol::task::{
    ConfigPreferencesResolution, TaskResolveConfigPreferencesParams,
};

impl TaskProductApi {
    pub(super) fn resolve_config_preferences(
        &self,
        client: &ClientInstanceId,
        params: TaskResolveConfigPreferencesParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str();
        let original = self.read_task_for_client(task_id, client)?;
        let preferences = self
            .store
            .read_agent_config_preferences(&original.agent_id)
            .map_err(runtime_error)?;
        let result = self
            .mutations
            .commit_existing_task(task_id, super::response_snapshot_options(), |ctx| {
                let task = ctx.task_mut();
                // Revalidate lease and binding inside the mutation; another client or a newer
                // recovery cannot accept settings for this client after the read above.
                if task.tombstoned
                    || task.lifecycle != original.lifecycle
                    || !matches!(task.lifecycle, TaskLifecycle::Prepared { lease: Some(_) })
                    || task.agent_session_id != original.agent_session_id
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                let Some(state) = task.config_mutation.preferences.as_mut() else {
                    return Ok(TaskMutationResult::Rejected);
                };
                if state.state != AgentConfigPreferencesState::Failed {
                    return Ok(TaskMutationResult::Rejected);
                }
                state.state = match params.action {
                    ConfigPreferencesResolution::Retry => AgentConfigPreferencesState::Applying,
                    ConfigPreferencesResolution::UseCurrentSettings => {
                        AgentConfigPreferencesState::Settled
                    }
                };
                task.updated_at = crate::time::now_string();
                Ok(TaskMutationResult::Changed)
            })
            .map_err(runtime_error)?;
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(conflict_error("Preference recovery is no longer available"));
        }
        if params.action == ConfigPreferencesResolution::Retry {
            let sessions = self.native_sessions.clone();
            let task_id = task_id.to_string();
            let session_id = original
                .agent_session_id
                .ok_or_else(|| conflict_error("No active session"))?;
            std::thread::spawn(move || {
                if let Err(error) =
                    sessions.apply_initial_preferences(&task_id, &session_id, &preferences)
                {
                    crate::logging::warn(
                        "task_preferences_retry_failed",
                        serde_json::json!({
                            "task_id": task_id, "error_code": error.code(),
                        }),
                    );
                }
            });
        }
        self.project_task_snapshot(
            result
                .response_snapshot
                .ok_or_else(|| internal_error("Missing preference recovery snapshot"))?,
        )
    }
}
