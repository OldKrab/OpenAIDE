use crate::agent::AgentPromptOutcome;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, InterruptionReason, NormalizedMessage, TaskStatus,
};
use crate::storage::records::TaskRecord;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::helpers::{append_interruption, chat_commit_options};
use super::TaskTransitions;

impl TaskTransitions {
    pub(crate) fn active_turn_id(&self, task_id: &str) -> Result<Option<String>, RuntimeError> {
        let _guard = self.mutations.lock();
        Ok(self.mutations.store().read_task(task_id)?.active_turn_id)
    }

    /// Marks durable acceptance as running immediately before the Agent receives the prompt.
    pub(crate) fn mark_turn_running(
        &self,
        task_id: &str,
        turn_id: &str,
    ) -> Result<bool, RuntimeError> {
        let mut prompt_may_start = false;
        let result = self.mutations.commit_existing_task(
            task_id,
            crate::tasks::mutation::TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().active_turn_id.as_deref() != Some(turn_id) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                if ctx.task().status == TaskStatus::Active {
                    prompt_may_start = true;
                    return Ok(TaskMutationResult::Unchanged);
                }
                if ctx.task().status != TaskStatus::Starting {
                    return Ok(TaskMutationResult::Unchanged);
                }
                prompt_may_start = true;
                let task = ctx.task_mut();
                task.status = TaskStatus::Active;
                task.updated_at = now_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        let _ = result;
        Ok(prompt_may_start)
    }

    pub(crate) fn cancel_running_task(
        &self,
        task_id: &str,
        expected_turn_id: Option<&str>,
        message: &str,
        unread: bool,
    ) -> Result<bool, RuntimeError> {
        let result =
            self.mutations
                .commit_existing_task(task_id, chat_commit_options(), |ctx| {
                    if !active_turn_matches(ctx.task(), expected_turn_id) {
                        return Ok(TaskMutationResult::Unchanged);
                    }

                    let now = now_string();
                    ctx.finish_running_activities(ActivityStatus::Completed)?;
                    append_interruption(
                        ctx,
                        InterruptionReason::Canceled,
                        message,
                        now.clone(),
                        true,
                    )?;

                    let task = ctx.task_mut();
                    task.status = TaskStatus::Inactive;
                    task.active_turn_id = None;
                    task.unread |= unread;
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                })?;
        Ok(matches!(result.outcome, TaskCommitOutcome::Committed(_)))
    }

    pub(crate) fn finish_turn(
        &self,
        task_id: &str,
        turn_id: &str,
        result: Result<AgentPromptOutcome, RuntimeError>,
    ) -> Result<bool, RuntimeError> {
        let commit =
            self.mutations
                .commit_existing_task(task_id, chat_commit_options(), |ctx| {
                    if ctx.task().active_turn_id.as_deref() != Some(turn_id) {
                        return Ok(TaskMutationResult::Unchanged);
                    }

                    let now = now_string();
                    match &result {
                        Ok(AgentPromptOutcome::EndTurn | AgentPromptOutcome::Cancelled) => {
                            ctx.finish_running_activity(
                                &format!("turn:{turn_id}"),
                                ActivityStatus::Completed,
                            )?;
                            ctx.task_mut().status = TaskStatus::Inactive;
                        }
                        Ok(outcome) => {
                            ctx.finish_running_activity(
                                &format!("turn:{turn_id}"),
                                ActivityStatus::Completed,
                            )?;
                            append_prompt_outcome_activity(ctx, outcome, now.clone())?;
                            ctx.task_mut().status = TaskStatus::Inactive;
                        }
                        Err(error) => {
                            ctx.finish_running_activity(
                                &format!("turn:{turn_id}"),
                                ActivityStatus::Error,
                            )?;
                            append_interruption(
                                ctx,
                                InterruptionReason::Failed,
                                &error.to_string(),
                                now.clone(),
                                true,
                            )?;
                            ctx.task_mut().status = TaskStatus::Failed;
                        }
                    }

                    let task = ctx.task_mut();
                    task.active_turn_id = None;
                    task.unread = true;
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                })?;
        Ok(matches!(commit.outcome, TaskCommitOutcome::Committed(_)))
    }
}

fn append_prompt_outcome_activity(
    ctx: &crate::tasks::mutation::TaskMutationContext<'_>,
    outcome: &AgentPromptOutcome,
    created_at: String,
) -> Result<(), RuntimeError> {
    let message = match outcome {
        AgentPromptOutcome::MaxTokens => "The Agent reached its token limit.".to_string(),
        AgentPromptOutcome::MaxTurnRequests => "The Agent reached its request limit.".to_string(),
        AgentPromptOutcome::Refusal => "The Agent refused this request.".to_string(),
        AgentPromptOutcome::Other(reason) => format!("The Agent stopped: {reason}."),
        AgentPromptOutcome::EndTurn | AgentPromptOutcome::Cancelled => return Ok(()),
    };
    ctx.append_message(NormalizedMessage::Activity {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Agent stopped".to_string(),
        status: ActivityStatus::Error,
        created_at,
        collapsed: false,
        steps: vec![ActivityStep::Text {
            text: message,
            level: Some("error".to_string()),
        }],
    })
}

fn active_turn_matches(task: &TaskRecord, expected_turn_id: Option<&str>) -> bool {
    match (task.active_turn_id.as_deref(), expected_turn_id) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(active), Some(expected)) => active == expected,
    }
}
