use crate::agent::AgentPromptOutcome;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, ActivityStep, NormalizedMessage, TaskStatus};
use crate::storage::records::TaskAttentionReason;
use crate::tasks::attention::fresh_attention;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::active_work_end::apply_active_work_end;
use super::helpers::chat_commit_options;
use super::ActiveWorkEnd;
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
        self.mutations.commit_existing_task(
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
        Ok(prompt_may_start)
    }

    /// Records accepted Stop intent while retaining the active prompt identity.
    pub(crate) fn mark_turn_stopping(
        &self,
        task_id: &str,
        turn_id: &str,
    ) -> Result<bool, RuntimeError> {
        let mut stop_accepted = false;
        self.mutations.commit_existing_task(
            task_id,
            crate::tasks::mutation::TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().active_turn_id.as_deref() != Some(turn_id) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                stop_accepted = true;
                if ctx.task().status == TaskStatus::Stopping {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                task.status = TaskStatus::Stopping;
                task.updated_at = now_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(stop_accepted)
    }

    pub(crate) fn finish_turn(
        &self,
        task_id: &str,
        turn_id: &str,
        result: Result<AgentPromptOutcome, RuntimeError>,
    ) -> Result<bool, RuntimeError> {
        let mut active_work_ended = false;
        let mut active_work_end_name = None;
        let commit =
            self.mutations
                .commit_existing_task(task_id, chat_commit_options(), |ctx| {
                    if ctx.task().active_turn_id.as_deref() != Some(turn_id) {
                        return Ok(TaskMutationResult::Unchanged);
                    }

                    let now = now_string();
                    if ctx.task().status == TaskStatus::Stopping {
                        let cause = match &result {
                            Ok(_) => ActiveWorkEnd::UserStopped,
                            Err(error) => ActiveWorkEnd::CancellationFailed(error.to_string()),
                        };
                        active_work_end_name = Some(cause.name());
                        apply_active_work_end(ctx, &cause, now)?;
                        active_work_ended = true;
                        return Ok(TaskMutationResult::Changed);
                    }
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
                            let cause = ActiveWorkEnd::AgentFailed(error.to_string());
                            active_work_end_name = Some(cause.name());
                            apply_active_work_end(ctx, &cause, now)?;
                            active_work_ended = true;
                            return Ok(TaskMutationResult::Changed);
                        }
                    }

                    let task = ctx.task_mut();
                    task.active_turn_id = None;
                    task.active_turn_started_at = None;
                    task.unread = true;
                    task.attention = Some(fresh_attention(
                        match &result {
                            Ok(AgentPromptOutcome::EndTurn) => TaskAttentionReason::Finished,
                            Ok(
                                AgentPromptOutcome::Cancelled
                                | AgentPromptOutcome::MaxTokens
                                | AgentPromptOutcome::MaxTurnRequests
                                | AgentPromptOutcome::Refusal
                                | AgentPromptOutcome::Other(_),
                            ) => TaskAttentionReason::Stopped,
                            Err(_) => TaskAttentionReason::Failed,
                        },
                        now.clone(),
                    ));
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                })?;
        let committed = matches!(commit.outcome, TaskCommitOutcome::Committed(_));
        if committed && active_work_ended {
            self.close_task_requests(task_id);
            crate::logging::warn(
                "task_active_work_ended",
                serde_json::json!({
                    "task_id": task_id,
                    "cause": active_work_end_name,
                }),
            );
        }
        if committed {
            if let Err(error) = self.mutations.compact_message_journal(task_id) {
                crate::logging::warn(
                    "message_journal_compaction_failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "error": error.to_string(),
                    }),
                );
            }
        }
        Ok(committed)
    }
}

fn append_prompt_outcome_activity(
    ctx: &mut crate::tasks::mutation::TaskMutationContext<'_>,
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
