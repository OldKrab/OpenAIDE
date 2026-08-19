use std::time::Instant;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::TaskSetPermissionPolicyParams;

use crate::storage::records::TaskLifecycle;
use crate::tasks::mutation::TaskMutationResult;
use crate::time::now_string;

use super::{conflict_error, protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    /// Saves one user-owned policy without treating the change as Task activity.
    pub(super) fn set_task_permission_policy(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetPermissionPolicyParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.read_task_for_client(&task_id, client_instance_id)?;
        let started = Instant::now();
        crate::logging::info(
            "task_permission_policy_change_started",
            serde_json::json!({
                "task_id": task_id,
                "policy": permission_policy_name(params.policy),
            }),
        );
        let result = self.mutations.commit_existing_task(
            &task_id,
            super::response_snapshot_options(),
            |ctx| {
                if !matches!(ctx.task().lifecycle, TaskLifecycle::Open) {
                    return Err(crate::protocol::errors::RuntimeError::Conflict(
                        "Only Open Tasks can change permission handling".to_string(),
                    ));
                }
                if ctx.task().permission_policy == params.policy {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                task.permission_policy = params.policy;
                // Changing a preference must not make this Task look newly active in navigation.
                task.updated_at = now_string();
                Ok(TaskMutationResult::Changed)
            },
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                crate::logging::warn(
                    "task_permission_policy_change_completed",
                    serde_json::json!({
                        "task_id": task_id,
                        "outcome": "failed",
                        "duration_ms": started.elapsed().as_millis(),
                    }),
                );
                return Err(protocol_error_from_runtime(error));
            }
        };
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| conflict_error("Task permission handling changed concurrently"))?;
        let snapshot = self.project_task_snapshot(snapshot)?;
        crate::logging::info(
            "task_permission_policy_change_completed",
            serde_json::json!({
                "task_id": task_id,
                "policy": permission_policy_name(params.policy),
                "outcome": "succeeded",
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        Ok(snapshot)
    }
}

fn permission_policy_name(
    policy: openaide_app_server_protocol::snapshot::TaskPermissionPolicy,
) -> &'static str {
    match policy {
        openaide_app_server_protocol::snapshot::TaskPermissionPolicy::AskEveryTime => {
            "ask_every_time"
        }
        openaide_app_server_protocol::snapshot::TaskPermissionPolicy::AutoApprove => "auto_approve",
    }
}
