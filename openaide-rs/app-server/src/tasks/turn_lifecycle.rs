use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::agent::gateway::AgentGateway;
use crate::agent::registry::AgentRegistry;
use crate::agent::{AgentSession, AgentSessionResume, AgentSessionStart};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    InterruptionReason, NormalizedMessage, PermissionDecision, TaskSnapshot, TaskStatus,
};
use crate::protocol::params::{
    PermissionRespondParams, TaskCreateMode, TaskCreateParams, TaskIdParams,
};
use crate::storage::Store;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult, TaskMutations};
use crate::tasks::snapshot::build_snapshot;
use crate::tasks::transitions::TaskTransitions;
use crate::tasks::turns::TurnRunner;
use crate::time::now_string;

mod create;
mod prompt;

pub(crate) struct TaskTurnLifecycle {
    store: Store,
    store_update_lock: Arc<Mutex<()>>,
    mutations: TaskMutations,
    agent_gateway: AgentGateway,
    turn_runner: TurnRunner,
    agent_registry: AgentRegistry,
}

impl TaskTurnLifecycle {
    pub(crate) fn new(
        store: Store,
        store_update_lock: Arc<Mutex<()>>,
        mutations: TaskMutations,
        agent_gateway: AgentGateway,
        turn_runner: TurnRunner,
        agent_registry: AgentRegistry,
    ) -> Self {
        Self {
            store,
            store_update_lock,
            mutations,
            agent_gateway,
            turn_runner,
            agent_registry,
        }
    }

    pub(crate) fn create(&self, params: TaskCreateParams) -> Result<TaskSnapshot, RuntimeError> {
        if params.mode == TaskCreateMode::AdoptExternalSession {
            return self.create_adopted_session(params);
        }
        self.create_prompt_start(params)
    }

    pub(crate) fn cancel(&self, params: TaskIdParams) -> Result<TaskSnapshot, RuntimeError> {
        let Some(turn_id) = self.transitions().active_turn_id(&params.task_id)? else {
            return self.snapshot(params.task_id);
        };
        self.turn_runner.cancel_turn(&turn_id);

        self.transitions().cancel_running_task(
            &params.task_id,
            Some(&turn_id),
            "Task was stopped.",
            false,
        )?;
        self.snapshot(params.task_id)
    }

    pub(crate) fn respond_permission(
        &self,
        params: PermissionRespondParams,
    ) -> Result<TaskSnapshot, RuntimeError> {
        let now = now_string();
        let task_id = params.task_id.clone();
        let request_id = params.request_id.clone();
        let option_id = params.option_id.clone();
        let decision = params.decision;
        let result = self.turn_runner.route_permission_response(
            &request_id,
            option_id.clone(),
            |live_turn_waiting| {
                self.mutations.commit_existing_task(
                    &task_id,
                    snapshot_chat_commit_options(),
                    |ctx| {
                        ctx.resolve_permission(&request_id, &option_id, decision)?;
                        if ctx.task().status == TaskStatus::Blocked {
                            ctx.task_mut().status =
                                if live_turn_waiting && ctx.task().active_turn_id.is_some() {
                                    TaskStatus::Active
                                } else {
                                    TaskStatus::Inactive
                                };
                        }
                        if !live_turn_waiting && decision == PermissionDecision::Denied {
                            ctx.append_message(NormalizedMessage::Interruption {
                                id: Uuid::new_v4().to_string(),
                                reason: InterruptionReason::Canceled,
                                message: "Permission denied.".to_string(),
                                created_at: now.clone(),
                                recoverable: true,
                            })?;
                        }
                        let task = ctx.task_mut();
                        task.unread = false;
                        task.updated_at = now.clone();
                        task.last_activity = now;
                        Ok(TaskMutationResult::Changed)
                    },
                )
            },
        )?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| RuntimeError::Internal("missing permission snapshot".to_string()))?;
        Ok(snapshot)
    }

    pub(crate) fn shutdown(&self) -> Result<(), RuntimeError> {
        self.turn_runner.shutdown()
    }

    pub(crate) fn recover_volatile_runtime_state(&self) -> Result<(), RuntimeError> {
        self.transitions().recover_volatile_runtime_state()
    }

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        self.agent_gateway.start_session(request)
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        self.agent_gateway.resume_session(request)
    }

    fn attach_session_events(&self, task_id: String, session_id: &str) -> Result<(), RuntimeError> {
        self.turn_runner.attach_session_events(task_id, session_id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.store_update_lock
            .lock()
            .expect("store update lock poisoned")
    }

    fn snapshot(&self, task_id: String) -> Result<TaskSnapshot, RuntimeError> {
        let _guard = self.lock();
        build_snapshot(&self.store, &task_id, 100)
    }

    fn turn_is_still_active(&self, task_id: &str, turn_id: &str) -> Result<bool, RuntimeError> {
        let _guard = self.lock();
        Ok(self.store.read_task(task_id)?.active_turn_id.as_deref() == Some(turn_id))
    }

    fn fail_created_task_start(
        &self,
        task_id: &str,
        error: &RuntimeError,
    ) -> Result<(), RuntimeError> {
        self.transitions().fail_created_task_start(task_id, error)
    }

    fn fail_adopted_task_attach(
        &self,
        task_id: &str,
        session_id: &str,
        error: &RuntimeError,
    ) -> Result<(), RuntimeError> {
        self.transitions()
            .fail_adopted_task_attach(task_id, session_id, error)
    }

    fn transitions(&self) -> TaskTransitions {
        TaskTransitions::new(self.mutations.clone())
    }
}

fn snapshot_chat_commit_options() -> TaskCommitOptions {
    TaskCommitOptions {
        refresh_message_history: true,
        response_snapshot_tail_limit: Some(100),
    }
}

fn required_prompt_text(value: String, field: &str) -> Result<String, RuntimeError> {
    if value.trim().is_empty() {
        Err(RuntimeError::InvalidParams(field.to_string()))
    } else {
        Ok(value)
    }
}
