use crate::agent::AgentPromptOutcome;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, ChatMessage, NormalizedMessage, TaskStatus,
};
use crate::storage::records::{TaskAttentionReason, TaskMessageQueuePauseRecord};
use crate::tasks::attention::fresh_attention;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::active_work_end::apply_active_work_end;
use super::helpers::chat_commit_options;
use super::ActiveWorkEnd;
use super::TaskTransitions;

#[derive(Debug)]
pub(crate) struct QueuedTurnAcceptance {
    pub(crate) text: String,
    pub(crate) attachments: Vec<crate::protocol::model::Attachment>,
    pub(crate) turn_id: String,
}

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
        self.finish_turn_internal(task_id, turn_id, result, None)
            .map(|(committed, _)| committed)
    }

    /// Atomically settles a normal turn and accepts one queued head as its successor.
    pub(crate) fn finish_turn_and_accept_queue(
        &self,
        task_id: &str,
        turn_id: &str,
        result: Result<AgentPromptOutcome, RuntimeError>,
        next_turn_id: &str,
        next_message_id: &str,
    ) -> Result<Option<QueuedTurnAcceptance>, RuntimeError> {
        self.finish_turn_internal(
            task_id,
            turn_id,
            result,
            Some((next_turn_id, next_message_id)),
        )
        .map(|(_, queued)| queued)
    }

    fn finish_turn_internal(
        &self,
        task_id: &str,
        turn_id: &str,
        result: Result<AgentPromptOutcome, RuntimeError>,
        queued_successor: Option<(&str, &str)>,
    ) -> Result<(bool, Option<QueuedTurnAcceptance>), RuntimeError> {
        let mut active_work_ended = false;
        let mut active_work_end_name = None;
        let mut active_work_end_error = None;
        let mut queued_turn = None;
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
                        active_work_end_error = cause.diagnostic_message().map(str::to_string);
                        apply_active_work_end(ctx, &cause, now)?;
                        active_work_ended = true;
                        return Ok(TaskMutationResult::Changed);
                    }
                    match &result {
                        Ok(AgentPromptOutcome::EndTurn) => {
                            ctx.finish_running_activity(
                                &format!("turn:{turn_id}"),
                                ActivityStatus::Completed,
                            )?;
                            if let Some((next_turn_id, next_message_id)) = queued_successor {
                                if ctx.task().message_queue.pause.is_none() {
                                    if let Some(queued) =
                                        ctx.task().message_queue.items.first().cloned()
                                    {
                                        if !queued_attachments_available(ctx.task(), &queued) {
                                            let queue = &mut ctx.task_mut().message_queue;
                                            queue.pause = Some(
                                                TaskMessageQueuePauseRecord::AttachmentUnavailable,
                                            );
                                            queue.revision = queue.revision.saturating_add(1);
                                        } else {
                                            accept_queued_turn(
                                                ctx,
                                                task_id,
                                                &queued,
                                                next_turn_id,
                                                next_message_id,
                                                &now,
                                            )?;
                                            queued_turn = Some(QueuedTurnAcceptance {
                                                text: queued.text,
                                                attachments: queued.agent_attachments,
                                                turn_id: next_turn_id.to_string(),
                                            });
                                            return Ok(TaskMutationResult::Changed);
                                        }
                                    }
                                }
                            }
                            ctx.task_mut().status = TaskStatus::Inactive;
                        }
                        Ok(AgentPromptOutcome::Cancelled) => {
                            ctx.finish_running_activity(
                                &format!("turn:{turn_id}"),
                                ActivityStatus::Completed,
                            )?;
                            ctx.task_mut().status = TaskStatus::Inactive;
                            pause_queue_after_unsuccessful_turn(ctx.task_mut());
                        }
                        Ok(outcome) => {
                            ctx.finish_running_activity(
                                &format!("turn:{turn_id}"),
                                ActivityStatus::Completed,
                            )?;
                            append_prompt_outcome_activity(ctx, outcome, now.clone())?;
                            ctx.task_mut().status = TaskStatus::Inactive;
                            pause_queue_after_unsuccessful_turn(ctx.task_mut());
                        }
                        Err(error) => {
                            let cause = ActiveWorkEnd::AgentFailed(error.to_string());
                            active_work_end_name = Some(cause.name());
                            active_work_end_error = cause.diagnostic_message().map(str::to_string);
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
                    "error": active_work_end_error,
                }),
            );
        }
        if committed {
            if let Err(error) = self.mutations.maintain_task_storage(task_id) {
                crate::logging::warn(
                    "task_storage_maintenance_failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "error": error.to_string(),
                    }),
                );
            }
        }
        Ok((committed, queued_turn))
    }
}

fn pause_queue_after_unsuccessful_turn(task: &mut crate::storage::records::TaskRecord) {
    if task.message_queue.items.is_empty() {
        return;
    }
    task.message_queue.pause = Some(TaskMessageQueuePauseRecord::UnsuccessfulTurn);
    task.message_queue.revision = task.message_queue.revision.saturating_add(1);
}

fn accept_queued_turn(
    ctx: &mut crate::tasks::mutation::TaskMutationContext<'_>,
    task_id: &str,
    queued: &crate::storage::records::QueuedMessageRecord,
    turn_id: &str,
    message_id: &str,
    now: &str,
) -> Result<(), RuntimeError> {
    ctx.append_chat_message(ChatMessage {
        cursor: String::new(),
        identity: format!("user:{message_id}"),
        message_type: "user".to_string(),
        message_id: message_id.to_string(),
        message: NormalizedMessage::User {
            id: format!("user:{message_id}"),
            text: queued.text.clone(),
            created_at: now.to_string(),
            attachments: queued.chat_attachments.clone(),
        },
    });
    let mut activity = crate::tasks::lifecycle::running_turn_message(now);
    let NormalizedMessage::Activity { id, .. } = &mut activity else {
        return Err(RuntimeError::Internal(
            "running turn marker must be an activity".to_string(),
        ));
    };
    *id = format!("turn:{turn_id}");
    ctx.append_chat_message(ChatMessage {
        cursor: String::new(),
        identity: format!("turn:{turn_id}"),
        message_type: "activity".to_string(),
        message_id: format!("message_{}", uuid::Uuid::new_v4()),
        message: activity,
    });

    let task = ctx.task_mut();
    task.message_queue.items.remove(0);
    task.message_queue.revision = task.message_queue.revision.saturating_add(1);
    let project_id = crate::projects::project_id_for_workspace(
        task.project_root
            .as_deref()
            .unwrap_or(task.workspace_root.as_str()),
    );
    task.composer_history.record(
        crate::storage::composer_history::ComposerHistoryEntryRecord {
            entry_id: message_id.to_string(),
            project_id: project_id.as_str().to_string(),
            text: queued.text.clone(),
            accepted_at: now.to_string(),
        },
    );
    task.status = TaskStatus::Starting;
    task.active_turn_id = Some(turn_id.to_string());
    task.active_turn_started_at = Some(now.to_string());
    task.attention = None;
    task.updated_at = now.to_string();
    task.last_activity = now.to_string();
    let _ = task_id;
    Ok(())
}

fn queued_attachments_available(
    task: &crate::storage::records::TaskRecord,
    queued: &crate::storage::records::QueuedMessageRecord,
) -> bool {
    if !task.supports_image_input
        && queued
            .agent_attachments
            .iter()
            .any(|attachment| attachment.kind == "image")
    {
        return false;
    }
    crate::tasks::product_api::queued_attachment_paths_are_available(&queued.agent_attachments)
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
