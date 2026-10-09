use std::time::Instant;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::TaskStopBackgroundCommandParams;

use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult, TaskMutations};
use crate::time::now_string;

use super::{conflict_error, protocol_error_from_runtime, runtime_error, TaskProductApi};

impl TaskProductApi {
    /// Accepts a stop of one Background Work command. The Agent request runs
    /// off the request path, so a slow Agent never stalls other clients; its
    /// failure is published on the command instead of in this response.
    pub(super) fn stop_background_command(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskStopBackgroundCommandParams,
    ) -> Result<(), ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.read_interactive_task_for_client(&task_id, client_instance_id)?;
        let task = self.store.read_task(&task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&task)?;
        let Some(turn_id) = task.active_turn_id.clone() else {
            return Err(conflict_error("Task turn is not active"));
        };
        let stoppable = task.live_background_work().is_some_and(|work| {
            work.commands
                .iter()
                .any(|command| command.command_id == params.command_id && command.can_stop)
        });
        if !stoppable {
            return Err(conflict_error("Background command is not running"));
        }
        // A retry starts clean; only this attempt's own failure may mark it.
        set_stop_failed(&self.mutations, &task_id, &params.command_id, false)
            .map_err(protocol_error_from_runtime)?;

        let turn_runner = self.turn_runner.clone();
        let mutations = self.mutations.clone();
        let command_id = params.command_id;
        crate::logging::info(
            "task_background_command_stop_started",
            serde_json::json!({ "task_id": task_id, "turn_id": turn_id, "attempt": 1 }),
        );
        let spawned = std::thread::Builder::new()
            .name("openaide-background-command-stop".to_string())
            .spawn(move || {
                let started = Instant::now();
                let result = turn_runner.stop_background_command(&turn_id, &command_id);
                crate::logging::info(
                    "task_background_command_stop",
                    serde_json::json!({
                        "task_id": task_id,
                        "turn_id": turn_id,
                        "attempt": 1,
                        "outcome": if result.is_ok() { "accepted" } else { "failed" },
                        "error_code": result.as_ref().err().map(|error| error.code()),
                        "error_kind": result.as_ref().err().map(|error| error.reason()),
                        "duration_ms": started.elapsed().as_millis(),
                    }),
                );
                if result.is_err() {
                    if let Err(error) = set_stop_failed(&mutations, &task_id, &command_id, true) {
                        crate::logging::error(
                            "task_background_command_stop_failure_not_recorded",
                            serde_json::json!({
                                "task_id": task_id,
                                "error_code": error.code(),
                                "error_kind": error.reason(),
                            }),
                        );
                    }
                }
            });
        spawned
            .map(drop)
            .map_err(|_| super::internal_error("Background command stop could not start"))
    }
}

#[cfg(test)]
impl TaskProductApi {
    pub(crate) fn stop_background_command_for_test(
        &self,
        params: TaskStopBackgroundCommandParams,
    ) -> Result<(), ProtocolError> {
        self.stop_background_command(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }
}

/// Publishes whether the last stop of a still-live command failed.
fn set_stop_failed(
    mutations: &TaskMutations,
    task_id: &str,
    command_id: &str,
    stop_failed: bool,
) -> Result<(), crate::protocol::errors::RuntimeError> {
    mutations.commit_existing_task(task_id, TaskCommitOptions::metadata(), |ctx| {
        let Some(command) = ctx.task_mut().background_work.as_mut().and_then(|work| {
            work.commands
                .iter_mut()
                .find(|command| command.command_id == command_id)
        }) else {
            return Ok(TaskMutationResult::Unchanged);
        };
        if command.stop_failed == stop_failed {
            return Ok(TaskMutationResult::Unchanged);
        }
        command.stop_failed = stop_failed;
        ctx.task_mut().updated_at = now_string();
        Ok(TaskMutationResult::Changed)
    })?;
    Ok(())
}
