use std::sync::{Arc, Mutex};

use crate::agent::gateway::AgentGateway;
use crate::agent::registry::AgentRegistry;
use crate::agent::{
    AgentPromptOutcome, AgentSession, AgentSessionKey, AgentSessionResume, AgentSessionStart,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskSnapshot;
use crate::protocol::params::{TaskCreateMode, TaskCreateParams, TaskIdParams};
use crate::storage::Store;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutations};
use crate::tasks::snapshot::build_snapshot;
use crate::tasks::transitions::TaskTransitions;
use crate::tasks::turns::TurnRunner;

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
        let transitions = self.transitions();
        if !transitions.mark_turn_stopping(&params.task_id, &turn_id)? {
            return self.snapshot(params.task_id);
        }
        match self.turn_runner.cancel_turn(&turn_id) {
            Ok(true) => {}
            Ok(false) => {
                transitions.finish_turn(
                    &params.task_id,
                    &turn_id,
                    Ok(AgentPromptOutcome::Cancelled),
                )?;
            }
            Err(error) => {
                transitions.finish_turn(&params.task_id, &turn_id, Err(error))?;
            }
        }
        self.snapshot(params.task_id)
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

    fn attach_session_events(
        &self,
        task_id: String,
        session: &AgentSessionKey,
    ) -> Result<std::sync::Arc<crate::tasks::turn_events::TaskSessionEventSink>, RuntimeError> {
        self.turn_runner.attach_session_events(task_id, session)
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
