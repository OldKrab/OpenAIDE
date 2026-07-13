use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::agent::{
    AgentPrompt, AgentRuntime, AgentSession, AgentSessionEventSink, AgentSessionKey,
    TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestRuntime;
use crate::tasks::mutation::TaskMutations;
use crate::tasks::transitions::TaskTransitions;
use crate::tasks::turn_events::{TaskEventSink, TaskSessionEventSink};
use openaide_app_server_protocol::ids::TaskId;

#[derive(Clone)]
pub struct TurnRunner {
    agent: Arc<dyn AgentRuntime>,
    mutations: TaskMutations,
    active_turns: Arc<ActiveTurnRegistry>,
    server_requests: ServerRequestRuntime,
}

impl TurnRunner {
    pub(crate) fn new(mutations: TaskMutations, agent: Arc<dyn AgentRuntime>) -> Self {
        Self::new_with_server_requests(mutations, agent, ServerRequestRuntime::new())
    }

    pub(crate) fn new_with_server_requests(
        mutations: TaskMutations,
        agent: Arc<dyn AgentRuntime>,
        server_requests: ServerRequestRuntime,
    ) -> Self {
        Self {
            agent,
            mutations,
            active_turns: Arc::new(ActiveTurnRegistry::default()),
            server_requests,
        }
    }

    pub(crate) fn active_turn_is_live(&self, task_id: &str, turn_id: &str) -> bool {
        self.active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .get(turn_id)
            .is_some_and(|active| active.task_id == task_id)
    }

    pub(crate) fn spawn_agent_turn(
        &self,
        task_id: String,
        prompt_text: String,
        prompt_attachments: Vec<crate::protocol::model::Attachment>,
        turn_id: String,
        session: AgentSession,
        session_sink: Arc<TaskSessionEventSink>,
    ) {
        let runner = self.clone();
        let cancellation = TurnCancellation::new();
        let active = ActiveTurn {
            task_id: task_id.clone(),
            cancellation: cancellation.clone(),
            session: session.key(),
        };
        self.active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .insert(turn_id.clone(), active.clone());
        thread::spawn(move || {
            let _registration = TurnRegistration {
                turn_id: turn_id.clone(),
                active_turns: runner.active_turns.clone(),
            };
            if cancellation.is_cancelled() {
                if runner.turn_is_active(&task_id, &turn_id) {
                    let _ = runner.transitions().finish_turn(
                        &task_id,
                        &turn_id,
                        Ok(crate::agent::AgentPromptOutcome::Cancelled),
                    );
                }
                return;
            }
            if !runner.turn_is_active(&task_id, &turn_id) {
                return;
            }
            match runner.transitions().mark_turn_running(&task_id, &turn_id) {
                Ok(true) => {}
                Ok(false) => {
                    if cancellation.is_cancelled() {
                        let _ = runner.transitions().finish_turn(
                            &task_id,
                            &turn_id,
                            Ok(crate::agent::AgentPromptOutcome::Cancelled),
                        );
                    }
                    return;
                }
                Err(error) => {
                    let _ = runner
                        .transitions()
                        .finish_turn(&task_id, &turn_id, Err(error));
                    return;
                }
            }

            let sink = Arc::new(TaskEventSink::with_session_sink(
                runner.mutations.clone(),
                task_id.clone(),
                turn_id.clone(),
                session_sink,
                runner.server_requests.clone(),
                cancellation.clone(),
            ));
            let result = runner.agent.prompt(
                AgentPrompt {
                    agent_id: session.agent_id,
                    task_id: task_id.clone(),
                    session_id: session.session_id,
                    text: prompt_text,
                    attachments: prompt_attachments,
                    cancellation: cancellation.clone(),
                },
                sink.clone(),
            );
            let _ = runner.transitions().finish_turn(&task_id, &turn_id, result);
        });
    }

    /// Starts one cancellation without finalizing Task state before the prompt settles.
    pub fn cancel_turn(&self, turn_id: &str) -> Result<bool, RuntimeError> {
        if let Some(active) = self
            .active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .get(turn_id)
            .cloned()
        {
            active.cancellation.cancel();
            self.server_requests.interrupt_task_requests(
                &TaskId::from(active.task_id),
                crate::client_lifecycle::AppServerTime::now(),
            );
            self.agent.cancel_session(&active.session)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn detach_stuck_turn(&self, turn_id: &str) {
        if let Some(active) = self
            .active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .remove(turn_id)
        {
            self.active_turns.changed.notify_all();
            active.cancellation.cancel();
            let _ = self.agent.cancel_session(&active.session);
        }
    }

    pub(crate) fn attach_session_events(
        &self,
        task_id: String,
        session: &AgentSessionKey,
    ) -> Result<Arc<TaskSessionEventSink>, RuntimeError> {
        let sink = Arc::new(TaskSessionEventSink::new(
            self.mutations.clone(),
            task_id,
            session.session_id().to_string(),
            self.server_requests.clone(),
        ));
        self.agent
            .attach_session_event_sink(session, sink.clone() as Arc<dyn AgentSessionEventSink>)?;
        Ok(sink)
    }

    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        let active_turns = self
            .active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .iter()
            .map(|(turn_id, active)| (turn_id.clone(), active.clone()))
            .collect::<Vec<_>>();

        for (_, active) in &active_turns {
            active.cancellation.cancel();
        }
        // Shutdown is best-effort across all live Tasks. One persistence failure
        // must not strand later Tasks or prevent the Agent runtime from closing.
        let mut first_error = None;
        for (turn_id, active) in active_turns {
            if let Err(error) = self.finalize_shutdown_turn(&active.task_id, &turn_id) {
                first_error.get_or_insert(error);
            }
        }

        if let Err(error) = self.agent.shutdown() {
            first_error.get_or_insert(error);
        }
        if let Err(error) = self.wait_for_active_turns_to_exit() {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }

    pub(crate) fn active_turns(&self) -> HashSet<(String, String)> {
        self.active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .iter()
            .map(|(turn_id, active)| (active.task_id.clone(), turn_id.clone()))
            .collect()
    }

    fn turn_is_active(&self, task_id: &str, turn_id: &str) -> bool {
        let _guard = self.mutations.lock();
        self.mutations
            .store()
            .read_task(task_id)
            .map(|task| task.active_turn_id.as_deref() == Some(turn_id))
            .unwrap_or(false)
    }

    fn finalize_shutdown_turn(&self, task_id: &str, turn_id: &str) -> Result<(), RuntimeError> {
        self.transitions()
            .cancel_running_task(
                task_id,
                Some(turn_id),
                "Task was stopped because OpenAIDE shut down.",
                true,
            )
            .map(|_| ())
    }

    fn transitions(&self) -> TaskTransitions {
        TaskTransitions::new(self.mutations.clone())
    }

    fn wait_for_active_turns_to_exit(&self) -> Result<(), RuntimeError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut active_turns = self
            .active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned");
        while !active_turns.is_empty() {
            let now = Instant::now();
            if now >= deadline {
                return Err(RuntimeError::Internal(
                    "timed out waiting for active turns to stop during shutdown".to_string(),
                ));
            }
            let (next, timeout) = self
                .active_turns
                .changed
                .wait_timeout(active_turns, deadline.saturating_duration_since(now))
                .expect("active turn registry wait poisoned");
            active_turns = next;
            if timeout.timed_out() && !active_turns.is_empty() {
                return Err(RuntimeError::Internal(
                    "timed out waiting for active turns to stop during shutdown".to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct ActiveTurnRegistry {
    turns: Mutex<HashMap<String, ActiveTurn>>,
    changed: Condvar,
}

struct TurnRegistration {
    turn_id: String,
    active_turns: Arc<ActiveTurnRegistry>,
}

impl Drop for TurnRegistration {
    fn drop(&mut self) {
        self.active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .remove(&self.turn_id);
        self.active_turns.changed.notify_all();
    }
}

#[derive(Clone)]
struct ActiveTurn {
    task_id: String,
    cancellation: TurnCancellation,
    session: AgentSessionKey,
}

#[cfg(test)]
#[path = "turns_tests.rs"]
mod tests;
