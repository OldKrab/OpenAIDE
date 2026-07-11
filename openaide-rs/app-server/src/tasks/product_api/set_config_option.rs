use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::task::TaskSetConfigOptionParams;

use crate::agent::AgentSessionSetConfigOptionRequest;
use crate::protocol::model::ConfigOptionsCatalog;
use crate::snapshots::task_snapshot::project_stored_task_snapshot;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::tasks::snapshot::build_snapshot;
use crate::time::now_string;

use super::{
    conflict_error, internal_error, protocol_error_from_runtime, runtime_error, validation_error,
    TaskProductApi,
};

impl TaskProductApi {
    pub(super) fn set_config_option_on_task(
        &self,
        params: TaskSetConfigOptionParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        if params.config_id.as_str().trim().is_empty() {
            return Err(validation_error("configId", "Config option id is required"));
        }
        let task = self.store.read_task(&task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&task)?;
        super::prepare::reject_if_preparation_not_ready(&task)?;
        if task.config_options.get(params.config_id.as_str()) == Some(&params.value) {
            let snapshot =
                build_snapshot(&self.store, &task_id, 100).map_err(super::storage_error)?;
            return project_stored_task_snapshot(snapshot);
        }

        let now = now_string();
        let config_id = params.config_id.into_string();
        let value = params.value;
        let live_catalog = if let Some(session_id) = task.agent_session_id.clone() {
            Some(
                self.agent_gateway
                    .set_session_config_option(AgentSessionSetConfigOptionRequest {
                        agent_id: task.agent_id.clone(),
                        session_id,
                        config_id: config_id.clone(),
                        value: value.clone(),
                    })
                    .map_err(protocol_error_from_runtime)?,
            )
        } else {
            None
        };
        let result = self
            .mutations
            .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                if ctx.task().tombstoned {
                    return Ok(TaskMutationResult::Rejected);
                }
                if super::prepare::reject_if_preparation_not_ready(ctx.task()).is_err() {
                    return Ok(TaskMutationResult::Rejected);
                }
                let task = ctx.task_mut();
                if let Some(catalog) = live_catalog.clone() {
                    replace_task_config_catalog(task, catalog);
                } else {
                    task.config_options.insert(config_id.clone(), value.clone());
                    update_catalog_current_value(task, &config_id, &value);
                }
                if let Some(catalog) = &task.config_options_catalog {
                    task.model_id = catalog.model_id();
                }
                task.updated_at = now;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(conflict_error(
                "Task config changed before it could be stored",
            ));
        }
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing task config snapshot"))?;
        project_stored_task_snapshot(snapshot)
    }
}

fn replace_task_config_catalog(
    task: &mut crate::storage::records::TaskRecord,
    catalog: ConfigOptionsCatalog,
) {
    task.config_options = catalog.current_values();
    task.config_options_catalog = Some(catalog);
}

fn update_catalog_current_value(
    task: &mut crate::storage::records::TaskRecord,
    config_id: &str,
    value: &str,
) {
    let Some(catalog) = &mut task.config_options_catalog else {
        return;
    };
    if let Some(option) = catalog
        .options
        .iter_mut()
        .find(|option| option.id == config_id)
    {
        option.current_value = value.to_string();
    }
}
