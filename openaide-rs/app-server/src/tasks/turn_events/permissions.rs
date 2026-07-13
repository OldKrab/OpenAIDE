use crate::agent::events::{AgentEvent, AgentPermissionOutcome, AgentPermissionRequest};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{PermissionDecision, TaskStatus};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};
use crate::time::now_string;
use serde_json::json;

use super::TaskEventSink;

impl TaskEventSink {
    pub(super) fn handle_permission_request(
        &self,
        request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        // Opening the request and persisting its Chat row form one ordered emission.
        // The human wait below must not hold this lock: Agents can continue sending
        // output and tool progress while a permission decision is pending.
        let emission_guard = self
            .session_sink
            .emission_lock
            .lock()
            .expect("event sink lock poisoned");
        self.session_sink.finish_anonymous_text_routes();
        let request_id = request.request_id.clone();
        let Some(server_request_id) = self.server_requests.open_permission_request(
            &self.task_id,
            &request,
            Vec::new(),
            crate::client_lifecycle::AppServerTime(0),
        )?
        else {
            return Ok(AgentPermissionOutcome::Cancelled);
        };
        logging::info(
            "task_permission_request_opened",
            json!({
                "task_id": self.task_id.as_str(),
                "turn_id": self.turn_id.as_str(),
                "agent_request_id": request_id.as_str(),
                "server_request_id": server_request_id.as_str(),
                "tool_call_id": request.tool_call.tool_call_id.as_str(),
                "tool_kind": request.tool_call.kind.as_deref(),
                "option_count": request.options.len(),
            }),
        );

        let now = now_string();
        let mut message =
            crate::agent::normalizer::normalize_event(AgentEvent::PermissionRequest(request), &now);
        if let crate::protocol::model::NormalizedMessage::Permission {
            app_server_request_id,
            ..
        } = &mut message
        {
            *app_server_request_id = Some(server_request_id.as_str().to_string());
        }
        if let Err(error) = self.append_agent_message(message, &now, Some(TaskStatus::Blocked)) {
            self.server_requests.interrupt_request(
                &server_request_id,
                crate::client_lifecycle::AppServerTime(0),
            );
            return Err(error);
        }
        drop(emission_guard);

        logging::info(
            "task_permission_wait_start",
            json!({
                "task_id": self.task_id.as_str(),
                "turn_id": self.turn_id.as_str(),
                "agent_request_id": request_id.as_str(),
                "server_request_id": server_request_id.as_str(),
            }),
        );
        let response = self
            .server_requests
            .wait_permission_response(&server_request_id, &self.cancellation)?;
        logging::info(
            "task_permission_wait_end",
            json!({
                "task_id": self.task_id.as_str(),
                "turn_id": self.turn_id.as_str(),
                "agent_request_id": request_id.as_str(),
                "server_request_id": server_request_id.as_str(),
                "outcome": agent_permission_outcome_name(&response.outcome),
                "has_decision": response.decision.is_some(),
            }),
        );
        if let AgentPermissionOutcome::Selected { option_id } = &response.outcome {
            let _guard = self
                .session_sink
                .emission_lock
                .lock()
                .expect("event sink lock poisoned");
            if let Err(error) = self.resolve_permission(&request_id, option_id, response.decision) {
                if !is_permission_already_resolved(&error) {
                    return Err(error);
                }
            }
        }
        Ok(response.outcome)
    }

    fn resolve_permission(
        &self,
        request_id: &str,
        option_id: &str,
        decision: Option<PermissionDecision>,
    ) -> Result<(), RuntimeError> {
        let now = now_string();
        let decision = decision.ok_or_else(|| {
            RuntimeError::InvalidParams("missing permission decision".to_string())
        })?;
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                ctx.resolve_permission(request_id, option_id, decision)?;
                if ctx.task().status == TaskStatus::Blocked {
                    ctx.task_mut().status = TaskStatus::Active;
                }
                let task = ctx.task_mut();
                task.unread = false;
                task.updated_at = now.clone();
                task.last_activity = now.clone();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        logging::info(
            "task_permission_resolved",
            json!({
                "task_id": self.task_id.as_str(),
                "turn_id": self.turn_id.as_str(),
                "agent_request_id": request_id,
                "option_id": option_id,
            }),
        );
        Ok(())
    }
}

fn agent_permission_outcome_name(outcome: &AgentPermissionOutcome) -> &'static str {
    match outcome {
        AgentPermissionOutcome::Selected { .. } => "selected",
        AgentPermissionOutcome::Cancelled => "cancelled",
    }
}

fn is_permission_already_resolved(error: &RuntimeError) -> bool {
    matches!(error, RuntimeError::InvalidParams(message) if message == "permission already resolved")
}
