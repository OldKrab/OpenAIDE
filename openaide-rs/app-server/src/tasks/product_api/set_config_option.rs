use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::TaskSetConfigOptionParams;

use crate::agent::AgentSessionSetConfigOptionRequest;
use crate::tasks::config_options::{
    apply_stored_config_option, apply_task_config_mutation_result, begin_task_config_mutation,
    clear_task_config_mutation, TaskConfigMutationToken,
};
use crate::tasks::mutation::TaskMutationResult;
use crate::tasks::snapshot::build_snapshot;
use crate::time::now_string;

use super::{
    conflict_error, internal_error, protocol_error_from_runtime, runtime_error, validation_error,
    TaskProductApi,
};

impl TaskProductApi {
    pub(super) fn set_config_option_on_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetConfigOptionParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.read_task_for_client(&task_id, client_instance_id)?;
        self.config_operations.serialize(&task_id, || {
            self.set_config_option_on_task_serialized(&task_id, params)
        })
    }

    fn set_config_option_on_task_serialized(
        &self,
        task_id: &str,
        params: TaskSetConfigOptionParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        if params.config_id.as_str().trim().is_empty() {
            return Err(validation_error("configId", "Config option id is required"));
        }
        if params.client_mutation_id.as_str().trim().is_empty() {
            return Err(validation_error(
                "clientMutationId",
                "Client mutation id is required",
            ));
        }
        let task = self.store.read_task(task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&task)?;
        super::prepare::reject_if_preparation_not_ready(&task)?;
        if task.config_mutation.pending.is_none()
            && task.config_options.get(params.config_id.as_str()) == Some(&params.value)
        {
            let snapshot =
                build_snapshot(&self.store, task_id, 100).map_err(super::storage_error)?;
            return self.project_task_snapshot(snapshot);
        }

        let now = now_string();
        let config_id = params.config_id.into_string();
        let value = params.value;
        let client_mutation_id = params.client_mutation_id.into_string();
        let expected_session_id = task.agent_session_id.clone();
        let Some(session_id) = expected_session_id.clone() else {
            return self
                .set_stored_config_option_without_session(task_id, &config_id, &value, &now);
        };
        let Some(mutation) = self.begin_live_config_mutation(
            task_id,
            expected_session_id.as_deref(),
            client_mutation_id,
            config_id.clone(),
            value.clone(),
            &now,
        )?
        else {
            let snapshot =
                build_snapshot(&self.store, task_id, 100).map_err(super::storage_error)?;
            return self.project_task_snapshot(snapshot);
        };
        let live_catalog =
            match self
                .agent_gateway
                .set_session_config_option(AgentSessionSetConfigOptionRequest {
                    agent_id: task.agent_id.clone(),
                    session_id,
                    config_id,
                    value,
                }) {
                Ok(catalog) => catalog,
                Err(error) => {
                    self.clear_failed_live_config_mutation(task_id, &mutation.token)?;
                    return Err(protocol_error_from_runtime(error));
                }
            };
        self.finish_live_config_mutation(
            task_id,
            expected_session_id.as_deref(),
            mutation,
            live_catalog,
            &now_string(),
        )
    }

    fn begin_live_config_mutation(
        &self,
        task_id: &str,
        expected_session_id: Option<&str>,
        client_mutation_id: String,
        config_id: String,
        requested_value: String,
        now: &str,
    ) -> Result<Option<LiveConfigMutation>, ProtocolError> {
        let mut admission = LiveConfigMutationAdmission::Missing;
        self.mutations
            .commit_existing_task(
                task_id,
                crate::tasks::mutation::TaskCommitOptions::metadata(),
                |ctx| {
                    if !live_config_target_is_current(ctx.task(), expected_session_id) {
                        admission = LiveConfigMutationAdmission::TaskChanged;
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if let Some(pending) = &ctx.task().config_mutation.pending {
                        if pending.client_mutation_id == client_mutation_id {
                            if pending.config_id == config_id
                                && pending.requested_value == requested_value
                            {
                                admission = LiveConfigMutationAdmission::Duplicate;
                            } else {
                                admission = LiveConfigMutationAdmission::ClientMutationIdReused;
                            }
                            return Ok(TaskMutationResult::Unchanged);
                        }
                    }
                    let admitted_config_options = ctx.task().config_options.clone();
                    let admitted_catalog = ctx.task().config_options_catalog.clone();
                    let task = ctx.task_mut();
                    let token = begin_task_config_mutation(
                        task,
                        client_mutation_id,
                        config_id,
                        requested_value,
                    )?;
                    admission = LiveConfigMutationAdmission::Started(LiveConfigMutation {
                        token,
                        admitted_config_options,
                        admitted_catalog,
                    });
                    task.updated_at = now.to_string();
                    Ok(TaskMutationResult::Changed)
                },
            )
            .map_err(protocol_error_from_runtime)?;
        match admission {
            LiveConfigMutationAdmission::Started(mutation) => Ok(Some(mutation)),
            LiveConfigMutationAdmission::Duplicate => Ok(None),
            LiveConfigMutationAdmission::ClientMutationIdReused => Err(conflict_error(
                "Client mutation id already identifies another config change",
            )),
            LiveConfigMutationAdmission::TaskChanged => Ok(None),
            LiveConfigMutationAdmission::Missing => {
                Err(internal_error("missing Task config mutation admission"))
            }
        }
    }

    fn clear_failed_live_config_mutation(
        &self,
        task_id: &str,
        token: &TaskConfigMutationToken,
    ) -> Result<(), ProtocolError> {
        let now = now_string();
        self.mutations
            .commit_existing_task(
                task_id,
                crate::tasks::mutation::TaskCommitOptions::metadata(),
                |ctx| {
                    Ok(if clear_task_config_mutation(ctx.task_mut(), token, &now) {
                        TaskMutationResult::Changed
                    } else {
                        TaskMutationResult::Unchanged
                    })
                },
            )
            .map_err(protocol_error_from_runtime)?;
        Ok(())
    }

    fn finish_live_config_mutation(
        &self,
        task_id: &str,
        expected_session_id: Option<&str>,
        mutation: LiveConfigMutation,
        catalog: crate::protocol::model::ConfigOptionsCatalog,
        now: &str,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let result = self
            .mutations
            .commit_existing_task(task_id, super::response_snapshot_options(), |ctx| {
                let newer_catalog_exists = ctx.task().config_options
                    != mutation.admitted_config_options
                    || ctx.task().config_options_catalog != mutation.admitted_catalog;
                if !live_config_target_is_current(ctx.task(), expected_session_id)
                    || newer_catalog_exists
                {
                    return Ok(
                        if clear_task_config_mutation(ctx.task_mut(), &mutation.token, now) {
                            TaskMutationResult::Changed
                        } else {
                            TaskMutationResult::Unchanged
                        },
                    );
                }
                Ok(
                    if apply_task_config_mutation_result(
                        ctx.task_mut(),
                        &mutation.token,
                        catalog,
                        now,
                    ) {
                        TaskMutationResult::Changed
                    } else {
                        // A newer server-ordered mutation or confirming session event owns state.
                        TaskMutationResult::Unchanged
                    },
                )
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing task config snapshot"))?;
        self.project_task_snapshot(snapshot)
    }

    fn set_stored_config_option_without_session(
        &self,
        task_id: &str,
        config_id: &str,
        value: &str,
        now: &str,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let result = self
            .mutations
            .commit_existing_task(task_id, super::response_snapshot_options(), |ctx| {
                if ctx.task().tombstoned
                    || super::prepare::reject_if_preparation_not_ready(ctx.task()).is_err()
                    || ctx.task().agent_session_id.is_some()
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                let task = ctx.task_mut();
                let pending_cleared = task.config_mutation.pending.take().is_some();
                let changed = apply_stored_config_option(task, config_id, value);
                if !changed && !pending_cleared {
                    return Ok(TaskMutationResult::Unchanged);
                }
                task.updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing stored task config snapshot"))?;
        self.project_task_snapshot(snapshot)
    }
}

enum LiveConfigMutationAdmission {
    Missing,
    Started(LiveConfigMutation),
    Duplicate,
    ClientMutationIdReused,
    TaskChanged,
}

struct LiveConfigMutation {
    token: TaskConfigMutationToken,
    // ACP projects the complete response catalog through the ordered session sink
    // before this request resumes. These values reveal whether Agent-owned state
    // already advanced; fallback runtimes that only return a catalog still use the
    // direct response commit.
    admitted_config_options: std::collections::HashMap<String, String>,
    admitted_catalog: Option<crate::protocol::model::ConfigOptionsCatalog>,
}

fn live_config_target_is_current(
    task: &crate::storage::records::TaskRecord,
    expected_session_id: Option<&str>,
) -> bool {
    !task.tombstoned
        && super::prepare::reject_if_preparation_not_ready(task).is_ok()
        && task.agent_session_id.as_deref() == expected_session_id
}
