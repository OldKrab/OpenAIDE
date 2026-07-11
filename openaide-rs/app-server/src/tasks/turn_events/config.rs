use crate::agent::TurnCancellation;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult, TaskMutations};

pub(super) struct ConfigUpdateTarget<'a> {
    pub(super) mutations: &'a TaskMutations,
    pub(super) task_id: &'a str,
}

pub(super) fn update_task_config_options(
    target: ConfigUpdateTarget<'_>,
    catalog: ConfigOptionsCatalog,
    now: &str,
    active_turn: Option<(&str, &TurnCancellation)>,
) -> Result<(), RuntimeError> {
    target.mutations.commit_existing_task(
        target.task_id,
        TaskCommitOptions::metadata(),
        |ctx| {
            if let Some((turn_id, cancellation)) = active_turn {
                if ctx.task().active_turn_id.as_deref() != Some(turn_id)
                    || cancellation.is_cancelled()
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
            }

            let task = ctx.task_mut();
            task.config_options = catalog.current_values();
            task.config_options_catalog = Some(catalog.clone());
            task.model_id = catalog.model_id();
            task.updated_at = now.to_string();
            Ok(TaskMutationResult::Changed)
        },
    )?;
    Ok(())
}
