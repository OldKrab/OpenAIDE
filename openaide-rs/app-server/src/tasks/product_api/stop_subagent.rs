use std::time::Instant;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::TaskStopSubagentParams;

use crate::agent::AgentSessionKey;
use crate::storage::subagents::SubagentStatusRecord;

use super::{conflict_error, runtime_error, TaskProductApi};

impl TaskProductApi {
    /// Sends a stop to one Running Native Subagent that advertises `cancel`.
    /// The Agent call runs off the request path so a busy session worker never
    /// stalls other clients. Its outcome is logged; the Subagent's status
    /// changes only through the Agent's own later report.
    pub(super) fn stop_subagent(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskStopSubagentParams,
    ) -> Result<(), ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let subagent_id = params.subagent_id.as_str().to_string();
        let task = self.read_interactive_task_for_client(&task_id, client_instance_id)?;
        let Some(agent_session_id) = task.agent_session_id.clone() else {
            return Err(conflict_error("Task has no Agent session"));
        };
        let record = self
            .store
            .subagent_catalog(&task_id)
            .map_err(runtime_error)?
            .entries
            .into_iter()
            .find(|entry| entry.subagent_id == subagent_id)
            .ok_or_else(|| {
                runtime_error(crate::protocol::errors::RuntimeError::TaskNotFound(
                    "subagent not found".to_string(),
                ))
            })?;
        if !matches!(
            record.status,
            SubagentStatusRecord::Running | SubagentStatusRecord::WaitingForActivity
        ) {
            return Err(conflict_error("Subagent is not running"));
        }
        if !record.capabilities.cancel {
            return Err(conflict_error("Subagent cannot be stopped"));
        }

        let session = AgentSessionKey::new(task.agent_id, agent_session_id);
        let agent_gateway = self.agent_gateway.clone();
        let native_session_id = record.native_session_id;
        crate::logging::info(
            "task_subagent_stop_started",
            serde_json::json!({ "task_id": task_id, "subagent_id": subagent_id, "attempt": 1 }),
        );
        let spawned = std::thread::Builder::new()
            .name("openaide-subagent-stop".to_string())
            .spawn(move || {
                let started = Instant::now();
                let result = agent_gateway.stop_subagent(&session, &native_session_id);
                crate::logging::info(
                    "task_subagent_stop",
                    serde_json::json!({
                        "task_id": task_id,
                        "subagent_id": subagent_id,
                        "attempt": 1,
                        "outcome": if result.is_ok() { "sent" } else { "failed" },
                        "error_code": result.as_ref().err().map(|error| error.code()),
                        "error_kind": result.as_ref().err().map(|error| error.reason()),
                        "duration_ms": started.elapsed().as_millis(),
                    }),
                );
            });
        spawned
            .map(drop)
            .map_err(|_| super::internal_error("Subagent stop could not start"))
    }
}

#[cfg(test)]
impl TaskProductApi {
    pub(crate) fn stop_subagent_for_test(
        &self,
        params: TaskStopSubagentParams,
    ) -> Result<(), ProtocolError> {
        self.stop_subagent(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }
}
