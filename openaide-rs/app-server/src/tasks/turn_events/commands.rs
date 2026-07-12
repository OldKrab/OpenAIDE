use crate::protocol::errors::RuntimeError;
use crate::protocol::model::AgentCommandsCatalog;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult, TaskMutations};

use super::CatalogUpdateSource;

pub(super) struct CommandsUpdateTarget<'a> {
    pub(super) mutations: &'a TaskMutations,
    pub(super) task_id: &'a str,
}

pub(super) fn update_task_commands(
    target: CommandsUpdateTarget<'_>,
    catalog: AgentCommandsCatalog,
    now: &str,
    source: CatalogUpdateSource<'_>,
) -> Result<(), RuntimeError> {
    target.mutations.commit_existing_task(
        target.task_id,
        TaskCommitOptions::metadata(),
        |ctx| {
            if !source.matches(ctx.task()) {
                return Ok(TaskMutationResult::Unchanged);
            }

            let task = ctx.task_mut();
            if task.agent_commands_catalog.as_ref() == Some(&catalog) {
                return Ok(TaskMutationResult::Unchanged);
            }
            task.agent_commands_catalog = Some(catalog);
            task.updated_at = now.to_string();
            Ok(TaskMutationResult::Changed)
        },
    )?;
    Ok(())
}
