use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::TaskClosePlanParams;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;
use crate::storage::records::TaskLifecycle;
use crate::tasks::mutation::TaskMutationResult;
use crate::time::now_string;

use super::{internal_error, protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    /// Atomically replaces the pinned current Plan with a durable user-closed Chat row.
    pub(super) fn close_task_plan(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskClosePlanParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.read_interactive_task_for_client(&task_id, client_instance_id)?;
        let now = now_string();
        let result = self
            .mutations
            .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                if !matches!(ctx.task().lifecycle, TaskLifecycle::Open) {
                    return Err(RuntimeError::Conflict(
                        "Only Open Tasks can close a Plan".to_string(),
                    ));
                }
                let Some(plan) = ctx.task().current_plan.clone() else {
                    return Err(RuntimeError::Conflict(
                        "Task has no current Plan to close".to_string(),
                    ));
                };
                let message_id = format!("user_closed_plan:{}", uuid::Uuid::new_v4());
                ctx.upsert_message_with_details(NormalizedMessage::ClosedPlan {
                    id: message_id,
                    entries: plan.entries,
                    created_at: now.clone(),
                })?;
                let task = ctx.task_mut();
                task.current_plan = None;
                task.completed_plan_message_id = None;
                task.updated_at = now.clone();
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing Task close-Plan snapshot"))?;
        crate::logging::info(
            "task_plan_closed_by_user",
            serde_json::json!({ "task_id": task_id }),
        );
        self.project_task_snapshot(snapshot)
    }
}
