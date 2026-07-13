use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;
use crate::tasks::config_options::apply_task_config_catalog;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult, TaskMutations};

use super::CatalogUpdateSource;

pub(super) struct ConfigUpdateTarget<'a> {
    pub(super) mutations: &'a TaskMutations,
    pub(super) task_id: &'a str,
}

pub(super) fn update_task_config_options(
    target: ConfigUpdateTarget<'_>,
    catalog: ConfigOptionsCatalog,
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

            if !apply_task_config_catalog(ctx.task_mut(), catalog, now) {
                return Ok(TaskMutationResult::Unchanged);
            }
            Ok(TaskMutationResult::Changed)
        },
    )?;
    Ok(())
}
