use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::{TaskHistorySyncSnapshot, TaskSnapshot};
use openaide_app_server_protocol::task::TaskReloadNativeSessionParams;

use crate::protocol::model::{AgentListedSession, TaskStatus};
use crate::storage::records::TaskLifecycle;
use crate::tasks::native_session_service::HistoryRefreshRequest;

use super::{conflict_error, protocol_error_from_runtime, runtime_error, TaskProductApi};

impl TaskProductApi {
    /// Replays Agent-owned state only after the user explicitly accepts the replacement.
    pub(super) fn reload_native_session(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskReloadNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        if params.client_mutation_id.as_str().trim().is_empty() {
            return Err(ProtocolError {
                code: ProtocolErrorCode::InvalidRequest,
                message: "clientMutationId is required".to_string(),
                recoverable: false,
                target: None,
            });
        }
        self.read_interactive_task_for_client(&task_id, client_instance_id)?;
        self.session_operations
            .serialize(&task_id, || self.reload_native_session_serialized(&task_id))
    }

    fn reload_native_session_serialized(
        &self,
        task_id: &str,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task = self.store.read_task(task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&task)?;
        if !matches!(task.lifecycle, TaskLifecycle::Open)
            || !matches!(task.status, TaskStatus::Inactive)
            || task.active_turn_id.is_some()
        {
            return Err(conflict_error(
                "Task must be idle before reloading its Native Session",
            ));
        }
        let Some(stored_session_id) = task.agent_session_id.clone() else {
            return Err(conflict_error("Task Native Session is not ready"));
        };
        let stale_session_id = stored_session_id.clone();
        let Some(requirement) = task.native_session_reload_requirement.clone() else {
            return Err(conflict_error("Task has no pending Native Session reload"));
        };
        let Some(native_time) = crate::time::activity_millis(&requirement.observed_activity_at)
        else {
            return Err(ProtocolError {
                code: ProtocolErrorCode::Conflict,
                message: "Task reload requirement is invalid".to_string(),
                recoverable: true,
                target: None,
            });
        };
        let native_updated_at = u128::try_from(native_time).map_err(|_| ProtocolError {
            code: ProtocolErrorCode::Conflict,
            message: "Task reload requirement is invalid".to_string(),
            recoverable: true,
            target: None,
        })?;
        let generation = self.history_sync.begin_interactive(task_id);
        self.publish_history_sync(task_id, TaskHistorySyncSnapshot::Syncing { generation });
        crate::logging::info(
            "native_session_reload_started",
            serde_json::json!({
                "task_id": task_id,
                "agent_id": task.agent_id,
                "generation": generation,
                "trigger": "user",
            }),
        );
        let result = self
            .native_sessions
            .refresh_history(HistoryRefreshRequest {
                task: task.clone(),
                stored_session_id: stored_session_id.clone(),
                native_session: AgentListedSession {
                    session_id: stored_session_id,
                    cwd: task.workspace_root.clone(),
                    title: None,
                    last_activity: Some(requirement.observed_activity_at.clone()),
                    updated_at: Some(requirement.observed_activity_at.clone()),
                },
                native_updated_at,
                refreshed_at: crate::time::now_string(),
                clear_reload_requirement_through: Some(requirement.observed_activity_at),
            })
            .map_err(protocol_error_from_runtime);
        self.history_sync.finish_interactive(task_id, generation);
        match result {
            Ok(Some(snapshot)) => {
                self.publish_history_sync(task_id, TaskHistorySyncSnapshot::Updated { generation });
                crate::logging::info(
                    "native_session_reload_completed",
                    serde_json::json!({
                        "task_id": task_id,
                        "generation": generation,
                        "outcome": "loaded",
                    }),
                );
                self.project_task_snapshot(snapshot)
            }
            Ok(None) => {
                self.publish_history_sync(
                    task_id,
                    self.history_sync.reload_available_snapshot(task_id),
                );
                Err(conflict_error(
                    "Task changed before its Native Session could reload",
                ))
            }
            Err(error) => {
                self.mark_native_session_recovery_stale(task_id, &stale_session_id);
                self.publish_history_sync(
                    task_id,
                    self.history_sync.reload_available_snapshot(task_id),
                );
                crate::logging::warn(
                    "native_session_reload_completed",
                    serde_json::json!({
                        "task_id": task_id,
                        "generation": generation,
                        "outcome": "failed",
                        "error_code": format!("{:?}", error.code),
                    }),
                );
                Err(error)
            }
        }
    }
}
