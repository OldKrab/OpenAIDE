use openaide_app_server_protocol::ids::TaskId;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, InterruptionReason, TaskStatus};
use crate::storage::records::TaskAttentionReason;
use crate::tasks::attention::fresh_attention;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationContext, TaskMutationResult};
use crate::time::now_string;

use super::helpers::{append_interruption, chat_commit_options};
use super::TaskTransitions;

/// Why active Agent work ended without a normal prompt outcome.
#[derive(Debug, Clone)]
pub(crate) enum ActiveWorkEnd {
    UserStopped,
    AgentFailed(String),
    AgentStartFailed(String),
    CancellationFailed(String),
    Restarted,
    Shutdown,
    SupportRecovery,
}

impl TaskTransitions {
    /// Applies the one durable cleanup path for interrupted active Agent work.
    pub(crate) fn end_active_work(
        &self,
        task_id: &str,
        expected_work_id: Option<&str>,
        cause: ActiveWorkEnd,
    ) -> Result<bool, RuntimeError> {
        let result =
            self.mutations
                .commit_existing_task(task_id, chat_commit_options(), |ctx| {
                    if !active_work_matches(ctx, expected_work_id) {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    apply_active_work_end(ctx, &cause, now_string())?;
                    Ok(TaskMutationResult::Changed)
                })?;
        let ended = matches!(result.outcome, TaskCommitOutcome::Committed(_));
        if ended {
            self.close_task_requests(task_id);
            crate::logging::warn(
                "task_active_work_ended",
                serde_json::json!({
                    "task_id": task_id,
                    "cause": cause.name(),
                }),
            );
        }
        Ok(ended)
    }

    pub(super) fn close_task_requests(&self, task_id: &str) {
        self.server_requests.interrupt_task_requests(
            &TaskId::from(task_id.to_string()),
            crate::client_lifecycle::AppServerTime::now(),
        );
    }
}

pub(super) fn apply_active_work_end(
    ctx: &mut TaskMutationContext<'_>,
    cause: &ActiveWorkEnd,
    now: String,
) -> Result<(), RuntimeError> {
    ctx.finish_running_activities(ActivityStatus::Interrupted)?;
    append_interruption(ctx, cause.reason(), &cause.message(), now.clone(), true)?;
    let task = ctx.task_mut();
    task.status = TaskStatus::Inactive;
    task.active_turn_id = None;
    task.active_turn_started_at = None;
    if cause.clears_session() {
        task.agent_session_id = None;
    }
    task.unread = true;
    task.attention = cause
        .attention_reason()
        .map(|reason| fresh_attention(reason, now.clone()));
    task.updated_at = now.clone();
    task.last_activity = now;
    Ok(())
}

fn active_work_matches(ctx: &TaskMutationContext<'_>, expected_work_id: Option<&str>) -> bool {
    match expected_work_id {
        Some(expected) => ctx.task().active_turn_id.as_deref() == Some(expected),
        None => {
            ctx.task().active_turn_id.is_some()
                || matches!(
                    ctx.task().status,
                    TaskStatus::Starting
                        | TaskStatus::Active
                        | TaskStatus::Stopping
                        | TaskStatus::Waiting
                )
        }
    }
}

impl ActiveWorkEnd {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::UserStopped => "user_stopped",
            Self::AgentFailed(_) => "agent_failed",
            Self::AgentStartFailed(_) => "agent_start_failed",
            Self::CancellationFailed(_) => "cancellation_failed",
            Self::Restarted => "restarted",
            Self::Shutdown => "shutdown",
            Self::SupportRecovery => "support_recovery",
        }
    }

    fn reason(&self) -> InterruptionReason {
        match self {
            Self::UserStopped | Self::Shutdown | Self::SupportRecovery => {
                InterruptionReason::Canceled
            }
            Self::Restarted => InterruptionReason::BackendUnavailable,
            Self::AgentFailed(_) | Self::AgentStartFailed(_) | Self::CancellationFailed(_) => {
                InterruptionReason::Failed
            }
        }
    }

    fn message(&self) -> String {
        match self {
            Self::UserStopped => "Task was stopped.".to_string(),
            Self::AgentFailed(message) | Self::AgentStartFailed(message) => {
                format!("Agent work stopped: {message}")
            }
            Self::CancellationFailed(message) => format!("Unable to stop the Agent: {message}"),
            Self::Restarted => "Task was interrupted because OpenAIDE restarted.".to_string(),
            Self::Shutdown => "Task was interrupted because OpenAIDE shut down.".to_string(),
            Self::SupportRecovery => {
                "Task was interrupted by support recovery because the Agent appeared stuck."
                    .to_string()
            }
        }
    }

    fn clears_session(&self) -> bool {
        matches!(
            self,
            Self::AgentStartFailed(_) | Self::CancellationFailed(_)
        )
    }

    fn attention_reason(&self) -> Option<TaskAttentionReason> {
        match self {
            Self::UserStopped => None,
            Self::AgentFailed(_)
            | Self::AgentStartFailed(_)
            | Self::CancellationFailed(_)
            | Self::Restarted
            | Self::Shutdown
            | Self::SupportRecovery => Some(TaskAttentionReason::Failed),
        }
    }
}
