use uuid::Uuid;

use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::server_requests::{
    QuestionRequestParams, QuestionRequestResponse,
};

use crate::agent::TurnCancellation;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{NormalizedMessage, QuestionAction, QuestionState, TaskStatus};
use crate::storage::records::TaskAttentionReason;
use crate::tasks::attention::{current_request_attention, request_attention};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};
use crate::time::now_string;

use super::TaskSessionEventSink;

impl TaskSessionEventSink {
    pub(super) fn append_question_error(&self, message: String) -> Result<(), RuntimeError> {
        let now = now_string();
        self.mutations
            .commit_existing_task(&self.task_id, chat_commit_options(), |ctx| {
                ctx.append_message(NormalizedMessage::Question {
                    id: Uuid::new_v4().to_string(),
                    request_id: Uuid::new_v4().to_string(),
                    message: "Question".to_string(),
                    fields: Vec::new(),
                    state: QuestionState::Error,
                    created_at: now.clone(),
                    action: None,
                    content: None,
                    error: Some(message.clone()),
                    resolution_message: None,
                })?;
                ctx.task_mut().updated_at = now.clone();
                Ok(TaskMutationResult::Changed)
            })?;
        Ok(())
    }

    pub(super) fn handle_question(
        &self,
        form: QuestionRequestParams,
        cancellation: TurnCancellation,
    ) -> Result<QuestionRequestResponse, RuntimeError> {
        let Some(request_id) = self
            .server_requests
            .open_question_request(&self.task_id, form.clone())?
        else {
            return Ok(QuestionRequestResponse::Cancel);
        };
        let waiting_result = self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    return Err(RuntimeError::InvalidParams(
                        "elicitation session is not bound to this Task".to_string(),
                    ));
                }
                let task = ctx.task_mut();
                task.status = TaskStatus::Waiting;
                task.unread = true;
                let now = now_string();
                task.attention = Some(request_attention(
                    request_id.as_str(),
                    TaskAttentionReason::NeedsAnswer,
                    now.clone(),
                ));
                task.updated_at = now;
                Ok(TaskMutationResult::Changed)
            },
        );
        if let Err(error) = waiting_result {
            self.server_requests
                .interrupt_request(&request_id, crate::client_lifecycle::AppServerTime(0));
            return Err(error);
        }

        let response = self
            .server_requests
            .wait_question_response(&request_id, &cancellation)?;
        let now = now_string();
        let stopped = cancellation.is_cancelled();
        let (state, action, content) = match &response {
            QuestionRequestResponse::Submit { content } => (
                QuestionState::Resolved,
                QuestionAction::Submit,
                Some(content.clone()),
            ),
            QuestionRequestResponse::Cancel => {
                (QuestionState::Cancelled, QuestionAction::Cancel, None)
            }
        };
        self.mutations
            .commit_existing_task(&self.task_id, chat_commit_options(), |ctx| {
                ctx.append_message(NormalizedMessage::Question {
                    id: Uuid::new_v4().to_string(),
                    request_id: request_id.as_str().to_string(),
                    message: form.message.clone(),
                    fields: form.fields.clone(),
                    state,
                    created_at: now.clone(),
                    action: Some(action),
                    content: content.clone(),
                    error: None,
                    resolution_message: stopped
                        .then(|| "Task stopped while a question was pending.".to_string()),
                })?;
                // Read broker state inside the ordered Task mutation. A request that
                // opens concurrently will mark the Task waiting after this commit.
                let has_pending_request = self
                    .server_requests
                    .has_pending_for_task(&TaskId::from(self.task_id.clone()));
                if !has_pending_request && !stopped && ctx.task().status == TaskStatus::Waiting {
                    ctx.task_mut().status = if ctx.task().active_turn_id.is_some() {
                        TaskStatus::Active
                    } else {
                        TaskStatus::Inactive
                    };
                }
                let task = ctx.task_mut();
                task.attention = current_request_attention(
                    &self.server_requests,
                    &self.task_id,
                    task.attention.as_ref(),
                    now.clone(),
                );
                task.updated_at = now.clone();
                task.last_activity = now.clone();
                Ok(TaskMutationResult::Changed)
            })?;
        Ok(response)
    }
}

fn chat_commit_options() -> TaskCommitOptions {
    TaskCommitOptions {
        refresh_message_history: true,
        response_snapshot_tail_limit: None,
    }
}
