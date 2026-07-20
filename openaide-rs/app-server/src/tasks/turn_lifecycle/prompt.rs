use uuid::Uuid;

use crate::agent::{AgentSessionResume, AgentSessionStart, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{Attachment, NormalizedMessage, TaskSnapshot, TaskStatus};
use crate::protocol::params::SessionPromptParams;
use crate::storage::records::TaskRecord;
use crate::tasks::lifecycle::running_turn_message;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::{required_prompt_text, snapshot_chat_commit_options, TaskTurnLifecycle};

impl TaskTurnLifecycle {
    pub(crate) fn prompt(&self, params: SessionPromptParams) -> Result<TaskSnapshot, RuntimeError> {
        let task_id = params.task_id;
        let prompt_text = required_prompt_text(params.text, "text")?;
        let prompt_attachments = params.prompt_attachments;
        let agent_prompt_attachments = prompt_attachments.clone();
        let message_id = params.message_id;
        let now = now_string();
        let turn_id = Uuid::new_v4().to_string();
        let session_plan = {
            let _guard = self.lock();
            let task = self.store.read_task(&task_id)?;
            if task.active_turn_id.is_some() {
                return Err(RuntimeError::InvalidParams(
                    "task already has an active turn".to_string(),
                ));
            }
            AgentSessionPlan::from_task(&task)
        };
        let (session, close_on_failure) = match session_plan {
            AgentSessionPlan::Start {
                agent_id,
                cwd,
                model_id,
                context,
            } => (
                self.start_session(AgentSessionStart {
                    agent_id,
                    task_id: task_id.clone(),
                    cwd,
                    model_id,
                    context,
                    cancellation: TurnCancellation::new(),
                    secret_resolver: None,
                })?,
                true,
            ),
            AgentSessionPlan::Resume {
                agent_id,
                session_id,
                cwd,
                model_id,
            } => (
                self.resume_session(AgentSessionResume {
                    agent_id,
                    task_id: task_id.clone(),
                    session_id,
                    cwd,
                    model_id,
                    cancellation: TurnCancellation::new(),
                    secret_resolver: None,
                })?,
                false,
            ),
        };
        let session_sink = match self.attach_session_events(task_id.clone(), &session.key()) {
            Ok(sink) => sink,
            Err(error) => {
                if close_on_failure {
                    let _ = self.agent_gateway.close_session(&session.key());
                }
                return Err(error);
            }
        };
        let commit =
            self.mutations
                .commit_existing_task(&task_id, snapshot_chat_commit_options(), |ctx| {
                    if ctx.task().active_turn_id.is_some() {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    ctx.append_message(NormalizedMessage::User {
                        id: message_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
                        text: prompt_text.clone(),
                        created_at: now.clone(),
                        attachments: prompt_attachments,
                    })?;
                    ctx.append_message(running_turn_message(&now))?;
                    let task = ctx.task_mut();
                    task.status = TaskStatus::Active;
                    task.active_turn_id = Some(turn_id.clone());
                    task.active_turn_started_at = Some(now.clone());
                    task.agent_session_id = Some(session.session_id.clone());
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                });
        let snapshot = match commit {
            Ok(result) => match result.outcome {
                TaskCommitOutcome::Committed(_) => result
                    .response_snapshot
                    .ok_or_else(|| RuntimeError::Internal("missing prompt snapshot".to_string()))?,
                TaskCommitOutcome::Rejected(_) => {
                    if close_on_failure {
                        let _ = self.agent_gateway.close_session(&session.key());
                    }
                    return Err(RuntimeError::InvalidParams(
                        "task already has an active turn".to_string(),
                    ));
                }
            },
            Err(error) => {
                if close_on_failure {
                    let _ = self.agent_gateway.close_session(&session.key());
                }
                return Err(error);
            }
        };
        if self.turn_is_still_active(&task_id, &turn_id)? {
            self.turn_runner.spawn_agent_turn(
                task_id,
                prompt_text,
                agent_prompt_attachments,
                turn_id,
                session,
                session_sink,
            );
        }
        Ok(snapshot)
    }
}

enum AgentSessionPlan {
    Start {
        agent_id: String,
        cwd: String,
        model_id: Option<String>,
        context: Vec<Attachment>,
    },
    Resume {
        agent_id: String,
        session_id: String,
        cwd: String,
        model_id: Option<String>,
    },
}

impl AgentSessionPlan {
    fn from_task(task: &TaskRecord) -> Self {
        let agent_id = task.agent_id.clone();
        let cwd = task.workspace_root.clone();
        let model_id = task.model_id.clone();
        match &task.agent_session_id {
            Some(session_id) => AgentSessionPlan::Resume {
                agent_id,
                session_id: session_id.clone(),
                cwd,
                model_id,
            },
            None => AgentSessionPlan::Start {
                agent_id,
                cwd,
                model_id,
                context: Vec::new(),
            },
        }
    }
}
