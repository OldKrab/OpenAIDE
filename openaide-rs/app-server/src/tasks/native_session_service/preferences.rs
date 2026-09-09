use std::time::Instant;

use openaide_app_server_protocol::snapshot::{
    AgentConfigPreferencesSnapshot, AgentConfigPreferencesState,
};

use super::*;
use crate::agent::AgentSessionSetConfigOptionRequest;
use crate::storage::agent_config_preferences::AgentConfigPreferences;
use crate::tasks::config_options::{
    apply_task_config_mutation_result, begin_task_config_mutation, clear_task_config_mutation,
};
use crate::tasks::config_preferences::{initial_state, ordered_option_ids, supports_preference};

impl NativeSessionService {
    /// The bound session is already published and subscribed. Use the same ordered catalog
    /// writer as user mutations, so an Agent notification cannot be overwritten by a late reply.
    pub(crate) fn apply_initial_preferences(
        &self,
        task_id: &str,
        session_id: &str,
        preferences: &AgentConfigPreferences,
    ) -> Result<(), RuntimeError> {
        let task = self.store.read_task(task_id)?;
        if !task
            .config_mutation
            .preferences
            .as_ref()
            .is_some_and(|state| state.state == AgentConfigPreferencesState::Applying)
        {
            return Ok(());
        }
        let operation_id = format!("preferences-{}", uuid::Uuid::new_v4());
        let started = Instant::now();
        crate::logging::info(
            "task_preferences_apply_started",
            serde_json::json!({
                "task_id": task_id, "operation_id": operation_id, "attempt": 1,
            }),
        );
        let result = self.apply_preferences_inner(task_id, session_id, preferences, &operation_id);
        crate::logging::info(
            "task_preferences_apply_completed",
            serde_json::json!({
                "task_id": task_id, "operation_id": operation_id,
                "outcome": match &result { Ok(true) => "settled", Ok(false) => "failed", Err(_) => "interrupted" },
                "duration_ms": started.elapsed().as_millis(), "attempt": 1,
                "error_code": result.as_ref().err().map(RuntimeError::code),
            }),
        );
        if result.is_err() {
            // A storage or lifecycle interruption must not leave a live retry stuck as Applying.
            let _ = self.mutate_preferences(task_id, session_id, |task| {
                task.config_mutation.pending = None;
                if let Some(state) = task.config_mutation.preferences.as_mut() {
                    state.state = AgentConfigPreferencesState::Failed;
                }
                Ok(())
            });
        }
        result.map(|_| ())
    }

    fn apply_preferences_inner(
        &self,
        task_id: &str,
        session_id: &str,
        preferences: &AgentConfigPreferences,
        operation_id: &str,
    ) -> Result<bool, RuntimeError> {
        // A preceding option can reset a dependent value. One bounded second pass
        // reconciles those dependencies; an unsettled catalog requires user recovery.
        let mut failed = false;
        'passes: for _ in 0..2 {
            let task = self.store.read_task(task_id)?;
            let ids = ordered_option_ids(task.config_options_catalog.as_ref());
            for id in ids {
                let Some(preference) = preferences
                    .options
                    .iter()
                    .find(|preference| preference.id == id)
                else {
                    continue;
                };
                let mut mutation = None;
                self.mutate_preferences(task_id, session_id, |task| {
                    let Some(option) = task
                        .config_options_catalog
                        .as_ref()
                        .and_then(|catalog| catalog.options.iter().find(|option| option.id == id))
                    else {
                        return Ok(());
                    };
                    if !supports_preference(option, preference)
                        || option.current_value == preference.value
                    {
                        return Ok(());
                    }
                    let baseline = task.config_options_catalog.clone();
                    let token = begin_task_config_mutation(
                        task,
                        operation_id.to_string(),
                        id.clone(),
                        preference.value.clone(),
                    )?;
                    mutation = Some((token, baseline));
                    Ok(())
                })?;
                let Some((token, baseline)) = mutation else {
                    continue;
                };
                let result = self.agent_gateway.set_session_config_option(
                    AgentSessionSetConfigOptionRequest {
                        agent_id: task.agent_id.clone(),
                        session_id: session_id.to_string(),
                        config_id: id.clone(),
                        value: preference.value.clone(),
                        diagnostic_operation_id: Some(operation_id.to_string()),
                    },
                );
                if let Err(error) = &result {
                    crate::logging::warn(
                        "task_preference_apply_failed",
                        serde_json::json!({
                            "task_id": task_id, "operation_id": operation_id, "error_code": error.code(),
                        }),
                    );
                    failed = true;
                }
                self.mutate_preferences(task_id, session_id, |task| {
                    let now = now_string();
                    match result {
                        Ok(catalog) if task.config_options_catalog == baseline => {
                            apply_task_config_mutation_result(task, &token, catalog, &now);
                        }
                        _ => {
                            clear_task_config_mutation(task, &token, &now);
                        }
                    }
                    Ok(())
                })?;
                if failed {
                    break 'passes;
                }
            }
            let latest = self.store.read_task(task_id)?;
            if initial_state(preferences, latest.config_options_catalog.as_ref())
                .is_none_or(|state| state.state == AgentConfigPreferencesState::Settled)
            {
                break;
            }
        }
        self.mutate_preferences(task_id, session_id, |task| {
            let remaining = initial_state(preferences, task.config_options_catalog.as_ref());
            failed |= remaining
                .as_ref()
                .is_some_and(|state| state.state == AgentConfigPreferencesState::Applying);
            task.config_mutation.preferences = Some(AgentConfigPreferencesSnapshot {
                state: if failed {
                    AgentConfigPreferencesState::Failed
                } else {
                    AgentConfigPreferencesState::Settled
                },
                skipped_count: remaining.map_or(0, |state| state.skipped_count),
            });
            Ok(())
        })?;
        Ok(!failed)
    }

    fn mutate_preferences(
        &self,
        task_id: &str,
        session_id: &str,
        change: impl FnOnce(&mut TaskRecord) -> Result<(), RuntimeError>,
    ) -> Result<(), RuntimeError> {
        let mut owns_session = false;
        self.mutations
            .commit_existing_task(task_id, TaskCommitOptions::metadata(), |ctx| {
                let task = ctx.task_mut();
                if task.tombstoned
                    || task.agent_session_id.as_deref() != Some(session_id)
                    || !matches!(
                        task.lifecycle,
                        crate::storage::records::TaskLifecycle::Prepared { lease: Some(_) }
                    )
                    || !task
                        .config_mutation
                        .preferences
                        .as_ref()
                        .is_some_and(|state| state.state == AgentConfigPreferencesState::Applying)
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                owns_session = true;
                let before = task.config_mutation.clone();
                let catalog = task.config_options_catalog.clone();
                change(task)?;
                if before == task.config_mutation && catalog == task.config_options_catalog {
                    return Ok(TaskMutationResult::Unchanged);
                }
                task.updated_at = now_string();
                Ok(TaskMutationResult::Changed)
            })?;
        // An Agent notification can settle the pending mutation before its reply.
        // A no-op commit is successful as long as this operation still owns the session.
        if !owns_session {
            return Err(RuntimeError::NotReady(
                "Preference application no longer owns this session".into(),
            ));
        }
        Ok(())
    }
}
