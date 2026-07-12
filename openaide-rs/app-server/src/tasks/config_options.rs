use std::collections::HashMap;

use serde_json::Value;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;
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
    requested_value: String,
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
    let config_options = catalog.current_values();
    let model_id = catalog.model_id();
    let resolves_pending = task
        .config_mutation
        .pending
        .as_ref()
        .is_some_and(|pending| {
            config_options.get(&pending.config_id) == Some(&pending.requested_value)
        });
    let changed = task.config_options != config_options
        || task.config_options_catalog.as_ref() != Some(&catalog)
        || task.model_id != model_id
        || resolves_pending;
    if !changed {
        return false;
    }
    task.config_options = config_options;
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

/// Applies config selected before a live Native Session exists.
pub(crate) fn apply_stored_config_option(
    task: &mut TaskRecord,
    config_id: &str,
    value: &str,
) -> bool {
    let previous_options = task.config_options.clone();
    let previous_catalog = task.config_options_catalog.clone();
    let previous_model_id = task.model_id.clone();

    task.config_options
        .insert(config_id.to_string(), value.to_string());
    if let Some(catalog) = &mut task.config_options_catalog {
        if let Some(option) = catalog
            .options
            .iter_mut()
            .find(|option| option.id == config_id)
        {
            option.current_value = value.to_string();
        }
        task.model_id = catalog.model_id();
    }
    task.config_options != previous_options
        || task.config_options_catalog != previous_catalog
        || task.model_id != previous_model_id
}

pub(crate) fn selected_config_options(
    value: Option<&Value>,
) -> Result<HashMap<String, String>, RuntimeError> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    if value.is_null() {
        return Ok(HashMap::new());
    }
    let object = value
        .as_object()
        .ok_or_else(|| RuntimeError::InvalidParams("config_options".to_string()))?;
    let mut selected = HashMap::new();
    for (key, value) in object {
        if key.trim().is_empty() {
            return Err(RuntimeError::InvalidParams("config_options".to_string()));
        }
        let Some(value) = value.as_str() else {
            return Err(RuntimeError::InvalidParams(format!("config_options.{key}")));
        };
        selected.insert(key.clone(), value.to_string());
    }
    Ok(selected)
}
