use crate::agent::gateway::AgentGateway;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskSnapshot;
use crate::protocol::params::{DeleteMode, TaskDeleteParams, TaskIdParams};
use crate::tasks::mutation::{
    TaskCommitOptions, TaskCommitOutcome, TaskMutationResult, TaskMutations,
};
use serde_json::Value;

#[derive(Clone)]
pub(crate) struct TaskCommands {
    mutations: TaskMutations,
    agent_gateway: AgentGateway,
}

impl TaskCommands {
    pub(crate) fn new(mutations: TaskMutations, agent_gateway: AgentGateway) -> Self {
        Self {
            mutations,
            agent_gateway,
        }
    }

    pub(crate) fn mark_read(&self, params: TaskIdParams) -> Result<TaskSnapshot, RuntimeError> {
        let task_id = params.task_id;
        let result = self.mutations.commit_existing_task(
            &task_id,
            TaskCommitOptions {
                response_snapshot_tail_limit: Some(100),
                ..TaskCommitOptions::metadata()
            },
            |ctx| {
                if !ctx.task().unread && ctx.task().attention.is_none() {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                task.unread = false;
                task.attention = None;
                Ok(TaskMutationResult::Changed)
            },
        )?;
        result
            .response_snapshot
            .ok_or_else(|| RuntimeError::Internal("missing mark_read snapshot".to_string()))
    }

    pub(crate) fn delete(&self, params: TaskDeleteParams) -> Result<Value, RuntimeError> {
        let task_id = params.task_id.clone();
        let mode = params.mode;

        let result = self.mutations.commit_existing_task(
            &task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                match mode {
                    DeleteMode::Archive => {
                        if ctx.task().archived {
                            return Ok(TaskMutationResult::Unchanged);
                        }
                        ctx.task_mut().archived = true;
                    }
                    DeleteMode::Restore => {
                        if !ctx.task().archived {
                            return Ok(TaskMutationResult::Unchanged);
                        }
                        ctx.task_mut().archived = false;
                    }
                    DeleteMode::Delete => {
                        if ctx.task().tombstoned {
                            return Ok(TaskMutationResult::Unchanged);
                        }
                        ctx.task_mut().tombstoned = true;
                    }
                }
                Ok(TaskMutationResult::Changed)
            },
        )?;

        if mode == DeleteMode::Delete {
            if let TaskCommitOutcome::Committed(facts) = result.outcome {
                let _ = self
                    .agent_gateway
                    .native_session_lifecycle()
                    .delete_bound_session(&facts.committed_task);
            }
        }
        Ok(serde_json::json!({ "task_id": params.task_id, "hidden": true }))
    }
}
