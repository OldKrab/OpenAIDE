use uuid::Uuid;

use openaide_app_server_protocol::server_requests::{
    QuestionRequestParams, QuestionRequestResponse,
};

use crate::agent::TurnCancellation;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{NormalizedMessage, QuestionState, TaskStatus};
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
        let request_id = self
            .server_requests
            .open_question_request(&self.task_id, form.clone())?;
        let now = now_string();
        let append_result =
            self.mutations
                .commit_existing_task(&self.task_id, chat_commit_options(), |ctx| {
                    if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                        return Err(RuntimeError::InvalidParams(
                            "elicitation session is not bound to this Task".to_string(),
                        ));
                    }
                    ctx.append_message(NormalizedMessage::Question {
                        id: Uuid::new_v4().to_string(),
                        request_id: request_id.as_str().to_string(),
                        message: form.message.clone(),
                        fields: form.fields.clone(),
                        state: QuestionState::Pending,
                        created_at: now.clone(),
                        action: None,
                        content: None,
                        error: None,
                    })?;
                    let task = ctx.task_mut();
                    task.status = TaskStatus::Blocked;
                    task.unread = true;
                    task.updated_at = now.clone();
                    task.last_activity = now.clone();
                    Ok(TaskMutationResult::Changed)
                });
        if let Err(error) = append_result {
            self.server_requests
                .interrupt_request(&request_id, crate::client_lifecycle::AppServerTime(0));
            return Err(error);
        }

        let response = self
            .server_requests
            .wait_question_response(&request_id, &cancellation)?;
        let now = now_string();
        self.mutations
            .commit_existing_task(&self.task_id, chat_commit_options(), |ctx| {
                let has_pending = ctx.resolve_question(request_id.as_str(), &response)?;
                if !has_pending && ctx.task().status == TaskStatus::Blocked {
                    ctx.task_mut().status = if ctx.task().active_turn_id.is_some() {
                        TaskStatus::Active
                    } else {
                        TaskStatus::Inactive
                    };
                }
                let task = ctx.task_mut();
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
