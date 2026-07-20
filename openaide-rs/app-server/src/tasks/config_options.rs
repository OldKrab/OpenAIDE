use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ConfigOptionCurrentValue, ConfigOptionsCatalog};
use crate::storage::records::{PendingTaskConfigChange, TaskRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskConfigMutationToken(PendingTaskConfigChange);

impl TaskConfigMutationToken {
    pub(crate) fn is_current_for(&self, task: &TaskRecord) -> bool {
        task.config_mutation.pending.as_ref() == Some(&self.0)
    }
}

/// Records the latest client intent before Agent I/O begins.
///
/// The server sequence, rather than the caller-provided identifier, defines
/// ordering. The client identifier remains the correlation identity projected
/// to App Shells while the operation is pending.
pub(crate) fn begin_task_config_mutation(
    task: &mut TaskRecord,
    client_mutation_id: String,
    config_id: String,
    requested_value: ConfigOptionCurrentValue,
) -> Result<TaskConfigMutationToken, RuntimeError> {
    let sequence = task
        .config_mutation
        .sequence
        .checked_add(1)
        .ok_or_else(|| {
            RuntimeError::Internal("Task config mutation sequence overflow".to_string())
        })?;
    let pending = PendingTaskConfigChange {
        sequence,
        client_mutation_id,
        config_id,
        requested_value,
    };
    task.config_mutation.sequence = sequence;
    task.config_mutation.pending = Some(pending.clone());
    Ok(TaskConfigMutationToken(pending))
}

/// Applies an Agent-owned catalog through the single Task config state writer.
/// An event that confirms the requested value also settles the pending change;
/// the later direct response then becomes an idempotent read of that state.
pub(crate) fn apply_task_config_catalog(
    task: &mut TaskRecord,
    catalog: ConfigOptionsCatalog,
    now: &str,
) -> bool {
    let model_id = catalog.model_id();
    let resolves_pending = task
        .config_mutation
        .pending
        .as_ref()
        .is_some_and(|pending| {
            catalog.options.iter().any(|option| {
                option.id == pending.config_id && option.current_value == pending.requested_value
            })
        });
    let changed = task.config_options_catalog.as_ref() != Some(&catalog)
        || task.model_id != model_id
        || resolves_pending;
    if !changed {
        return false;
    }
    task.config_options_catalog = Some(catalog);
    task.model_id = model_id;
    if resolves_pending {
        task.config_mutation.pending = None;
    }
    task.updated_at = now.to_string();
    true
}

pub(crate) fn apply_task_config_mutation_result(
    task: &mut TaskRecord,
    token: &TaskConfigMutationToken,
    catalog: ConfigOptionsCatalog,
    now: &str,
) -> bool {
    if !token.is_current_for(task) {
        return false;
    }
    let catalog_changed = apply_task_config_catalog(task, catalog, now);
    if task.config_mutation.pending.take().is_some() {
        task.updated_at = now.to_string();
        return true;
    }
    catalog_changed
}

pub(crate) fn clear_task_config_mutation(
    task: &mut TaskRecord,
    token: &TaskConfigMutationToken,
    now: &str,
) -> bool {
    if !token.is_current_for(task) {
        return false;
    }
    task.config_mutation.pending = None;
    task.updated_at = now.to_string();
    true
}
