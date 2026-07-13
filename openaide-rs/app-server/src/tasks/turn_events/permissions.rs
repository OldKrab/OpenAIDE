use crate::agent::events::{AgentPermissionOutcome, AgentPermissionRequest};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    PermissionDecision, TaskStatus, ToolPermissionDecision, ToolPermissionOutcome,
};
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;
use openaide_app_server_protocol::ids::TaskId;
use serde_json::json;

use super::TaskEventSink;

impl TaskEventSink {
    pub(super) fn handle_permission_request(
        &self,
        request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        // Opening the transient request and changing Task status form one ordered
        // emission. The human wait below must not hold this lock: Agents can continue
        // sending output and tool progress while a decision is pending.
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

        match self.mark_permission_waiting() {
            Ok(true) => {}
            Ok(false) => {
                self.server_requests.interrupt_request(
                    &server_request_id,
                    crate::client_lifecycle::AppServerTime(0),
                );
                return Ok(AgentPermissionOutcome::Cancelled);
            }
            Err(error) => {
                self.server_requests.interrupt_request(
                    &server_request_id,
                    crate::client_lifecycle::AppServerTime(0),
                );
                return Err(error);
            }
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
        let _guard = self
            .session_sink
            .emission_lock
            .lock()
            .expect("event sink lock poisoned");
        self.persist_permission_resolution(
            request,
            server_request_id.as_str(),
            request_id.as_str(),
            &response.outcome,
            response.decision,
        )?;
        Ok(response.outcome)
    }

    fn mark_permission_waiting(&self) -> Result<bool, RuntimeError> {
        let now = now_string();
        let result = self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().active_turn_id.as_deref() != Some(self.turn_id.as_str())
                    || self.cancellation.is_cancelled()
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                task.status = TaskStatus::Waiting;
                task.updated_at = now.clone();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(matches!(result.outcome, TaskCommitOutcome::Committed(_)))
    }

    fn persist_permission_resolution(
        &self,
        request: AgentPermissionRequest,
        server_request_id: &str,
        agent_request_id: &str,
        outcome: &AgentPermissionOutcome,
        resolved_decision: Option<PermissionDecision>,
    ) -> Result<(), RuntimeError> {
        let now = now_string();
        let stopped = self.cancellation.is_cancelled();
        let (option_id, option_label) = match outcome {
            AgentPermissionOutcome::Selected { option_id } => (
                Some(option_id.clone()),
                request
                    .options
                    .iter()
                    .find(|option| option.option_id == *option_id)
                    .map(|option| option.name.clone()),
            ),
            AgentPermissionOutcome::Cancelled => (None, None),
        };
        let decision = match outcome {
            AgentPermissionOutcome::Selected { .. } => match resolved_decision {
                Some(PermissionDecision::Approved) => ToolPermissionDecision::Approved,
                Some(PermissionDecision::Denied) => ToolPermissionDecision::Rejected,
                None => ToolPermissionDecision::Cancelled,
            },
            AgentPermissionOutcome::Cancelled => ToolPermissionDecision::Cancelled,
        };
        let activity_identity = format!(
            "acp_tool:{}:{}",
            self.session_sink.session_id, request.tool_call.tool_call_id
        );
        let tool_call_id = request.tool_call.tool_call_id;
        let permission_outcome = ToolPermissionOutcome {
            request_id: server_request_id.to_string(),
            decision,
            option_id,
            option_label,
            resolved_at: now.clone(),
        };
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if !ctx.record_tool_permission_outcome(
                    &activity_identity,
                    &tool_call_id,
                    permission_outcome,
                )? {
                    return Err(RuntimeError::Internal(format!(
                        "permission request {server_request_id} has no linked tool {tool_call_id}"
                    )));
                }
                // Read broker state inside the ordered Task mutation. A request that
                // opens concurrently will mark the Task waiting after this commit.
                let has_pending_request = self
                    .server_requests
                    .has_pending_for_task(&TaskId::from(self.task_id.clone()));
                if !has_pending_request && !stopped && ctx.task().status == TaskStatus::Waiting {
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
                "agent_request_id": agent_request_id,
                "outcome": agent_permission_outcome_name(outcome),
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
