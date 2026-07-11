use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::agent::{
    AgentPrompt, AgentRuntime, AgentSession, AgentSessionEventSink, TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestRuntime;
use crate::tasks::mutation::TaskMutations;
use crate::tasks::transitions::TaskTransitions;
use crate::tasks::turn_events::{TaskEventSink, TaskSessionEventSink};

mod steering;

use steering::SteeringState;

#[derive(Clone)]
pub struct TurnRunner {
    agent: Arc<dyn AgentRuntime>,
    mutations: TaskMutations,
    active_turns: Arc<Mutex<HashMap<String, ActiveTurn>>>,
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
            active_turns: Arc::new(Mutex::new(HashMap::new())),
            server_requests,
        }
    }

    pub(crate) fn active_turn_is_live(&self, task_id: &str, turn_id: &str) -> bool {
        self.active_turns
            .lock()
            .expect("active turn registry poisoned")
            .get(turn_id)
            .is_some_and(|active| active.task_id == task_id)
    }

    pub fn spawn_agent_turn(
        &self,
        task_id: String,
        prompt_text: String,
        prompt_attachments: Vec<crate::protocol::model::Attachment>,
        turn_id: String,
        session: AgentSession,
    ) {
        let runner = self.clone();
        let cancellation = TurnCancellation::new();
        let active = ActiveTurn {
            task_id: task_id.clone(),
            cancellation: cancellation.clone(),
            session_id: session.session_id.clone(),
            steering: Arc::new(Mutex::new(SteeringState::default())),
        };
        self.active_turns
            .lock()
            .expect("active turn registry poisoned")
            .insert(turn_id.clone(), active.clone());
        thread::spawn(move || {
            let _registration = TurnRegistration {
                turn_id: turn_id.clone(),
                active_turns: runner.active_turns.clone(),
            };
            if cancellation.is_cancelled() || !runner.turn_is_active(&task_id, &turn_id) {
                return;
            }

            let sink = Arc::new(TaskEventSink::new(
                runner.mutations.clone(),
                task_id.clone(),
                turn_id.clone(),
                runner.server_requests.clone(),
                cancellation.clone(),
            ));
            let result = runner.agent.prompt(
                AgentPrompt {
                    task_id: task_id.clone(),
                    session_id: session.session_id,
                    text: prompt_text,
                    attachments: prompt_attachments,
                    cancellation: cancellation.clone(),
                },
                sink.clone(),
            );
            let result = result.and(sink.finish());
            if !runner.wait_until_steering_finishes(&task_id, &turn_id, &active) {
                return;
            }
            let _ = runner.transitions().finish_turn(&task_id, &turn_id, result);
        });
    }

    pub fn cancel_turn(&self, turn_id: &str) {
        if let Some(active) = self
            .active_turns
            .lock()
            .expect("active turn registry poisoned")
            .get(turn_id)
            .cloned()
        {
            active.cancellation.cancel();
            let _ = self.agent.cancel_session(&active.session_id);
        }
    }

    pub(crate) fn detach_stuck_turn(&self, turn_id: &str) {
        if let Some(active) = self
            .active_turns
            .lock()
            .expect("active turn registry poisoned")
            .remove(turn_id)
        {
            active.cancellation.cancel();
            let _ = self.agent.cancel_session(&active.session_id);
        }
    }

    pub(crate) fn route_permission_response<T>(
        &self,
        request_id: &str,
        option_id: String,
        commit: impl FnOnce(bool) -> Result<T, RuntimeError>,
    ) -> Result<T, RuntimeError> {
        self.server_requests
            .route_agent_permission_response(request_id, option_id, commit)
    }

    pub fn attach_session_events(
        &self,
        task_id: String,
        session_id: &str,
    ) -> Result<(), RuntimeError> {
        let sink: Arc<dyn AgentSessionEventSink> = Arc::new(TaskSessionEventSink::new(
            self.mutations.clone(),
            task_id,
            session_id.to_string(),
            self.server_requests.clone(),
        ));
        self.agent.attach_session_event_sink(session_id, sink)
    }

    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        let active_turns = self
            .active_turns
            .lock()
            .expect("active turn registry poisoned")
            .iter()
            .map(|(turn_id, active)| (turn_id.clone(), active.clone()))
            .collect::<Vec<_>>();

        for (_, active) in &active_turns {
            active.cancellation.cancel();
        }
        for (turn_id, active) in active_turns {
            self.finalize_shutdown_turn(&active.task_id, &turn_id)?;
        }

        let shutdown_result = self.agent.shutdown();
        self.wait_for_active_turns_to_exit()?;
        shutdown_result
    }

    pub(crate) fn active_turn_count(&self) -> usize {
        self.active_turns
            .lock()
            .expect("active turn registry poisoned")
            .len()
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
        while self.active_turn_count() != 0 {
            if Instant::now() >= deadline {
                return Err(RuntimeError::Internal(
                    "timed out waiting for active turns to stop during shutdown".to_string(),
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

struct TurnRegistration {
    turn_id: String,
    active_turns: Arc<Mutex<HashMap<String, ActiveTurn>>>,
}

impl Drop for TurnRegistration {
    fn drop(&mut self) {
        self.active_turns
            .lock()
            .expect("active turn registry poisoned")
            .remove(&self.turn_id);
    }
}

#[derive(Clone)]
struct ActiveTurn {
    task_id: String,
    cancellation: TurnCancellation,
    session_id: String,
    steering: Arc<Mutex<SteeringState>>,
}
