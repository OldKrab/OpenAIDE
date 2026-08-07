use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
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
use crate::tasks::turn_acceptance::TurnAcceptanceCoordinator;
use crate::tasks::turn_events::{TaskEventSink, TaskSessionEventSink};
use openaide_app_server_protocol::ids::TaskId;

const DEFAULT_CANCEL_GRACE_PERIOD: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct TurnRunner {
    agent: Arc<dyn AgentRuntime>,
    mutations: TaskMutations,
    active_turns: Arc<ActiveTurnRegistry>,
    server_requests: ServerRequestRuntime,
    native_catalog: Option<crate::native_sessions::catalog::NativeSessionCatalog>,
    cancel_grace_period: Arc<Mutex<Duration>>,
    turn_acceptance: TurnAcceptanceCoordinator,
    #[cfg(test)]
    settlement_pause: Arc<(Mutex<TestSettlementPause>, Condvar)>,
}

#[cfg(test)]
#[derive(Default)]
struct TestSettlementPause {
    enabled: bool,
    reached: bool,
    released: bool,
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
        let storage_failures = mutations.store().task_journal().subscribe_failures();
        let runner = Self {
            agent,
            mutations,
            active_turns: Arc::new(ActiveTurnRegistry::default()),
            server_requests,
            native_catalog: None,
            cancel_grace_period: Arc::new(Mutex::new(DEFAULT_CANCEL_GRACE_PERIOD)),
            turn_acceptance: TurnAcceptanceCoordinator::default(),
            #[cfg(test)]
            settlement_pause: Arc::new((
                Mutex::new(TestSettlementPause::default()),
                Condvar::new(),
            )),
        };
        runner.start_storage_failure_monitor(storage_failures);
        runner
    }

    /// Attaches the durable Native Session metadata projection used by live session updates.
    pub(crate) fn with_native_catalog(
        mut self,
        native_catalog: crate::native_sessions::catalog::NativeSessionCatalog,
    ) -> Self {
        self.native_catalog = Some(native_catalog);
        self
    }

    /// Stops live Native Session work as soon as its Task can no longer be durably updated.
    fn start_storage_failure_monitor(
        &self,
        failures: std::sync::mpsc::Receiver<crate::storage::task_journal::TaskStorageFailure>,
    ) {
        let active_turns = Arc::downgrade(&self.active_turns);
        let agent = self.agent.clone();
        let server_requests = self.server_requests.clone();
        std::thread::Builder::new()
            .name("openaide-task-storage-failure".to_string())
            .spawn(move || {
                while let Ok(failure) = failures.recv() {
                    let Some(active_turns) = active_turns.upgrade() else {
                        return;
                    };
                    let affected = active_turns
                        .turns
                        .lock()
                        .expect("active turn registry poisoned")
                        .values()
                        .filter(|active| active.task_id == failure.task_id)
                        .cloned()
                        .collect::<Vec<_>>();
                    if affected.is_empty() {
                        continue;
                    }
                    crate::logging::error(
                        "task_storage_failure_cancel_started",
                        serde_json::json!({
                            "task_id": failure.task_id,
                            "active_turns": affected.len(),
                        }),
                    );
                    server_requests.interrupt_task_requests(
                        &TaskId::from(failure.task_id.clone()),
                        crate::client_lifecycle::AppServerTime::now(),
                    );
                    for active in affected {
                        active.cancellation.cancel();
                        if let Err(error) = agent.cancel_session(&active.session) {
                            crate::logging::error(
                                "task_storage_failure_cancel_failed",
                                serde_json::json!({
                                    "task_id": failure.task_id,
                                    "agent_id": active.session.agent_id(),
                                    "session_id": active.session.session_id(),
                                    "error_code": error.code(),
                                    "error_kind": error.reason(),
                                }),
                            );
                        }
                    }
                }
            })
            .expect("Task storage failure monitor must start");
    }

    #[cfg(test)]
    pub(crate) fn set_cancel_grace_period_for_test(&self, grace_period: Duration) {
        *self
            .cancel_grace_period
            .lock()
            .expect("cancel grace period poisoned") = grace_period;
    }

    #[cfg(test)]
    pub(crate) fn pause_next_settlement_for_test(&self) {
        let (state, _) = &*self.settlement_pause;
        *state.lock().expect("turn settlement pause poisoned") = TestSettlementPause {
            enabled: true,
            reached: false,
            released: false,
        };
    }

    #[cfg(test)]
    pub(crate) fn wait_for_settlement_pause_for_test(&self) {
        let (state, changed) = &*self.settlement_pause;
        let mut state = state.lock().expect("turn settlement pause poisoned");
        while !state.reached {
            state = changed.wait(state).expect("turn settlement pause poisoned");
        }
    }

    #[cfg(test)]
    pub(crate) fn release_settlement_for_test(&self) {
        let (state, changed) = &*self.settlement_pause;
        let mut state = state.lock().expect("turn settlement pause poisoned");
        state.released = true;
        changed.notify_all();
    }

    #[cfg(test)]
    fn pause_before_settlement_for_test(&self) {
        let (state, changed) = &*self.settlement_pause;
        let mut state = state.lock().expect("turn settlement pause poisoned");
        if !state.enabled {
            return;
        }
        state.reached = true;
        changed.notify_all();
        while !state.released {
            state = changed.wait(state).expect("turn settlement pause poisoned");
        }
        state.enabled = false;
    }

    pub(crate) fn active_turn_is_live(&self, task_id: &str, turn_id: &str) -> bool {
        self.active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .get(turn_id)
            .is_some_and(|active| active.task_id == task_id)
    }

    /// Shares the Task admission boundary with Send so prompt settlement and a
    /// concurrent user message have one authoritative order.
    pub(super) fn turn_acceptance(&self) -> TurnAcceptanceCoordinator {
        self.turn_acceptance.clone()
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
        crate::logging::info(
            "task_primary_prompt_runner_spawned",
            serde_json::json!({
                "task_id": task_id.as_str(),
                "turn_id": turn_id.as_str(),
                "agent_id": session.agent_id.as_str(),
                "session_id": session.session_id.as_str(),
            }),
        );
        let active = ActiveTurn {
            task_id: task_id.clone(),
            cancellation: cancellation.clone(),
            session: session.key(),
        };
        let registration_started_at = Instant::now();
        self.active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .insert(turn_id.clone(), active.clone());
        crate::logging::info(
            "task_primary_prompt_runner_registered",
            serde_json::json!({
                "task_id": task_id.as_str(),
                "turn_id": turn_id.as_str(),
                "registration_elapsed_ms": registration_started_at.elapsed().as_millis(),
            }),
        );
        let thread_scheduled_at = Instant::now();
        let thread_task_id = task_id.clone();
        let thread_turn_id = turn_id.clone();
        thread::spawn(move || {
            let runner_started = Instant::now();
            crate::logging::info(
                "task_primary_prompt_runner_started",
                serde_json::json!({
                    "task_id": task_id.as_str(),
                    "turn_id": turn_id.as_str(),
                }),
            );
            let _registration = TurnRegistration {
                turn_id: turn_id.clone(),
                active_turns: runner.active_turns.clone(),
            };
            if cancellation.is_cancelled() {
                crate::logging::info(
                    "task_primary_prompt_runner_cancelled_before_running",
                    serde_json::json!({
                        "task_id": task_id.as_str(),
                        "turn_id": turn_id.as_str(),
                        "elapsed_ms": runner_started.elapsed().as_millis(),
                    }),
                );
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
                crate::logging::warn(
                    "task_primary_prompt_runner_not_active_before_running",
                    serde_json::json!({
                        "task_id": task_id.as_str(),
                        "turn_id": turn_id.as_str(),
                        "elapsed_ms": runner_started.elapsed().as_millis(),
                    }),
                );
                return;
            }
            crate::logging::info(
                "task_primary_prompt_runner_mark_running_started",
                serde_json::json!({
                    "task_id": task_id.as_str(),
                    "turn_id": turn_id.as_str(),
                    "elapsed_ms": runner_started.elapsed().as_millis(),
                }),
            );
            match runner.transitions().mark_turn_running(&task_id, &turn_id) {
                Ok(true) => crate::logging::info(
                    "task_primary_prompt_runner_marked_running",
                    serde_json::json!({
                        "task_id": task_id.as_str(),
                        "turn_id": turn_id.as_str(),
                        "elapsed_ms": runner_started.elapsed().as_millis(),
                    }),
                ),
                Ok(false) => {
                    crate::logging::info(
                        "task_primary_prompt_runner_mark_running_rejected",
                        serde_json::json!({
                            "task_id": task_id.as_str(),
                            "turn_id": turn_id.as_str(),
                            "elapsed_ms": runner_started.elapsed().as_millis(),
                        }),
                    );
                    // Stop is persisted before its in-memory cancellation token is
                    // signalled. If startup observes that durable Stopping window,
                    // it still owns finalization even when the token is not set yet.
                    if runner.turn_is_active(&task_id, &turn_id) {
                        let _ = runner.transitions().finish_turn(
                            &task_id,
                            &turn_id,
                            Ok(crate::agent::AgentPromptOutcome::Cancelled),
                        );
                    }
                    return;
                }
                Err(error) => {
                    crate::logging::error(
                        "task_primary_prompt_runner_mark_running_failed",
                        serde_json::json!({
                            "task_id": task_id.as_str(),
                            "turn_id": turn_id.as_str(),
                            "elapsed_ms": runner_started.elapsed().as_millis(),
                            "error": error.to_string(),
                        }),
                    );
                    let _ = runner
                        .transitions()
                        .finish_turn(&task_id, &turn_id, Err(error));
                    return;
                }
            }

            let next_session = session.clone();
            let next_session_sink = session_sink.clone();
            let sink = Arc::new(TaskEventSink::with_session_sink(
                runner.mutations.clone(),
                task_id.clone(),
                turn_id.clone(),
                session_sink,
                runner.server_requests.clone(),
                cancellation.clone(),
            ));
            crate::logging::info(
                "task_primary_prompt_agent_invoke_started",
                serde_json::json!({
                    "task_id": task_id.as_str(),
                    "turn_id": turn_id.as_str(),
                    "agent_id": session.agent_id.as_str(),
                    "session_id": session.session_id.as_str(),
                }),
            );
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
            let outcome = match &result {
                Ok(crate::agent::AgentPromptOutcome::EndTurn) => "end_turn",
                Ok(crate::agent::AgentPromptOutcome::MaxTokens) => "max_tokens",
                Ok(crate::agent::AgentPromptOutcome::MaxTurnRequests) => "max_turn_requests",
                Ok(crate::agent::AgentPromptOutcome::Refusal) => "refusal",
                Ok(crate::agent::AgentPromptOutcome::Cancelled) => "cancelled",
                Ok(crate::agent::AgentPromptOutcome::Other(_)) => "other",
                Err(_) => "error",
            };
            crate::logging::info(
                "task_primary_prompt_agent_invoke_returned",
                serde_json::json!({
                    "task_id": task_id.as_str(),
                    "turn_id": turn_id.as_str(),
                    "outcome": outcome,
                    "elapsed_ms": runner_started.elapsed().as_millis(),
                    "error_code": result.as_ref().err().map(RuntimeError::code),
                    "error_reason": result.as_ref().err().map(RuntimeError::reason),
                }),
            );
            let settlement_started_at = Instant::now();
            crate::logging::info(
                "task_primary_prompt_settlement_started",
                serde_json::json!({
                    "task_id": task_id.as_str(),
                    "turn_id": turn_id.as_str(),
                    "elapsed_ms": runner_started.elapsed().as_millis(),
                }),
            );
            let settlement_result = runner.turn_acceptance.serialize(&task_id, || {
                #[cfg(test)]
                runner.pause_before_settlement_for_test();
                let next_turn_id = format!("turn_{}", uuid::Uuid::new_v4());
                let next_message_id = format!("message_{}", uuid::Uuid::new_v4());
                if !runner
                    .turn_acceptance
                    .own_pending_turn(&task_id, &next_turn_id)
                {
                    return runner.transitions().finish_turn(&task_id, &turn_id, result);
                }
                let accepted = runner.transitions().finish_turn_and_accept_queue(
                    &task_id,
                    &turn_id,
                    result,
                    &next_turn_id,
                    &next_message_id,
                );
                match accepted {
                    Ok(Some(queued)) => {
                        runner.spawn_agent_turn(
                            task_id.clone(),
                            queued.text,
                            queued.attachments,
                            queued.turn_id.clone(),
                            next_session,
                            next_session_sink,
                        );
                        runner
                            .turn_acceptance
                            .retire_pending_turn(&task_id, &queued.turn_id);
                        Ok(true)
                    }
                    Ok(None) => {
                        runner
                            .turn_acceptance
                            .retire_pending_turn(&task_id, &next_turn_id);
                        Ok(true)
                    }
                    Err(error) => {
                        runner
                            .turn_acceptance
                            .retire_pending_turn(&task_id, &next_turn_id);
                        Err(error)
                    }
                }
            });
            crate::logging::info(
                "task_primary_prompt_settlement_returned",
                serde_json::json!({
                    "task_id": task_id.as_str(),
                    "turn_id": turn_id.as_str(),
                    "outcome": if settlement_result.is_ok() { "ok" } else { "error" },
                    "settlement_elapsed_ms": settlement_started_at.elapsed().as_millis(),
                    "elapsed_ms": runner_started.elapsed().as_millis(),
                    "error_code": settlement_result.as_ref().err().map(RuntimeError::code),
                    "error_reason": settlement_result.as_ref().err().map(RuntimeError::reason),
                }),
            );
        });
        crate::logging::info(
            "task_primary_prompt_runner_thread_spawned",
            serde_json::json!({
                "task_id": thread_task_id.as_str(),
                "turn_id": thread_turn_id.as_str(),
                "registration_to_thread_spawned_ms": thread_scheduled_at.elapsed().as_millis(),
            }),
        );
    }

    /// Forwards a steering prompt without registering another Task work lifecycle.
    pub(crate) fn steer_session(&self, prompt: AgentPrompt) -> Result<(), RuntimeError> {
        self.agent.steer(prompt)
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
            let dispatch_started = Instant::now();
            crate::logging::info(
                "task_cancel_accepted",
                serde_json::json!({
                    "task_id": active.task_id.as_str(),
                    "turn_id": turn_id,
                    "agent_id": active.session.agent_id(),
                    "session_id": active.session.session_id(),
                    "boundary": "turn_runner",
                }),
            );
            active.cancellation.cancel();
            self.spawn_cancel_watchdog(turn_id.to_string(), active.clone());
            self.server_requests.interrupt_task_requests(
                &TaskId::from(active.task_id.clone()),
                crate::client_lifecycle::AppServerTime::now(),
            );
            if let Err(error) = self.agent.cancel_session(&active.session) {
                crate::logging::error(
                    "task_cancel_runtime_dispatch_failed",
                    serde_json::json!({
                        "task_id": active.task_id.as_str(),
                        "turn_id": turn_id,
                        "agent_id": active.session.agent_id(),
                        "session_id": active.session.session_id(),
                        "boundary": "agent_runtime",
                        "dispatch_ms": dispatch_started.elapsed().as_millis(),
                        "error_code": error.code(),
                        "error_kind": error.reason(),
                    }),
                );
                return Err(error);
            }
            crate::logging::info(
                "task_cancel_runtime_dispatch_completed",
                serde_json::json!({
                    "task_id": active.task_id.as_str(),
                    "turn_id": turn_id,
                    "agent_id": active.session.agent_id(),
                    "session_id": active.session.session_id(),
                    "boundary": "agent_runtime",
                    "dispatch_ms": dispatch_started.elapsed().as_millis(),
                }),
            );
            return Ok(true);
        }
        Ok(false)
    }

    /// Gives a compliant Agent time to settle the prompt, then severs a session that no
    /// longer has a trustworthy lifecycle. The watchdog is independent of the cancel RPC so
    /// a blocked Agent adapter cannot leave the durable Task in `stopping` forever.
    fn spawn_cancel_watchdog(&self, turn_id: String, active: ActiveTurn) {
        let grace_period = *self
            .cancel_grace_period
            .lock()
            .expect("cancel grace period poisoned");
        let runner = self.clone();
        thread::spawn(move || {
            if runner.wait_for_turn_exit(&turn_id, grace_period) {
                return;
            }
            runner.force_end_timed_out_cancel(&turn_id, &active, grace_period);
        });
    }

    /// Returns true when the prompt settled before the cancellation deadline.
    fn wait_for_turn_exit(&self, turn_id: &str, timeout: Duration) -> bool {
        let turns = self
            .active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned");
        let (turns, _) = self
            .active_turns
            .changed
            .wait_timeout_while(turns, timeout, |turns| turns.contains_key(turn_id))
            .expect("active turn registry wait poisoned");
        !turns.contains_key(turn_id)
    }

    fn force_end_timed_out_cancel(
        &self,
        turn_id: &str,
        expected: &ActiveTurn,
        grace_period: Duration,
    ) {
        let removed = {
            let mut turns = self
                .active_turns
                .turns
                .lock()
                .expect("active turn registry poisoned");
            let matches = turns.get(turn_id).is_some_and(|active| {
                active.task_id == expected.task_id && active.session == expected.session
            });
            if matches {
                turns.remove(turn_id)
            } else {
                None
            }
        };
        let Some(active) = removed else {
            return;
        };
        self.active_turns.changed.notify_all();

        crate::logging::warn(
            "task_cancel_timed_out",
            serde_json::json!({
                "task_id": active.task_id.as_str(),
                "turn_id": turn_id,
                "agent_id": active.session.agent_id(),
                "session_id": active.session.session_id(),
                "grace_period_ms": grace_period.as_millis(),
            }),
        );
        if let Err(error) = self.transitions().end_active_work(
            &active.task_id,
            Some(turn_id),
            crate::tasks::transitions::ActiveWorkEnd::CancellationFailed(format!(
                "Agent did not confirm cancellation within {} ms; its live Native Session handle was closed while the Task binding was preserved",
                grace_period.as_millis()
            )),
        ) {
            crate::logging::error(
                "task_cancel_timeout_transition_failed",
                serde_json::json!({
                    "task_id": active.task_id.as_str(),
                    "turn_id": turn_id,
                    "error": error.to_string(),
                }),
            );
        }
        // Durable invalidation comes first so even a broken runtime adapter cannot keep the
        // Task in `stopping`. The ACP implementation still bounds close internally.
        let close_started = Instant::now();
        crate::logging::warn(
            "task_cancel_force_close_started",
            serde_json::json!({
                "task_id": active.task_id.as_str(),
                "turn_id": turn_id,
                "agent_id": active.session.agent_id(),
                "session_id": active.session.session_id(),
                "grace_period_ms": grace_period.as_millis(),
            }),
        );
        match self.agent.close_session(&active.session) {
            Ok(()) => crate::logging::info(
                "task_cancel_force_close_completed",
                serde_json::json!({
                    "task_id": active.task_id.as_str(),
                    "turn_id": turn_id,
                    "agent_id": active.session.agent_id(),
                    "session_id": active.session.session_id(),
                    "close_ms": close_started.elapsed().as_millis(),
                }),
            ),
            Err(error) => crate::logging::error(
                "task_cancel_timeout_session_close_failed",
                serde_json::json!({
                    "task_id": active.task_id.as_str(),
                    "turn_id": turn_id,
                    "agent_id": active.session.agent_id(),
                    "session_id": active.session.session_id(),
                    "close_ms": close_started.elapsed().as_millis(),
                    "error_code": error.code(),
                    "error_kind": error.reason(),
                }),
            ),
        }
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
        let sink = Arc::new(
            TaskSessionEventSink::new(
                self.mutations.clone(),
                task_id,
                session.session_id().to_string(),
                self.server_requests.clone(),
            )
            .with_native_catalog(self.native_catalog.clone()),
        );
        self.agent
            .attach_session_event_sink(session, sink.clone() as Arc<dyn AgentSessionEventSink>)?;
        Ok(sink)
    }

    /// Reattaches the Task-owned sink after a Native Session worker was closed and resumed.
    pub(crate) fn reattach_session_events(
        &self,
        session: &AgentSessionKey,
        sink: &Arc<TaskSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.agent
            .attach_session_event_sink(session, sink.clone() as Arc<dyn AgentSessionEventSink>)
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

    /// Settles every active turn without shutting down the reusable Agent runtime.
    pub(crate) fn settle_for_task_history_reset(&self) -> Result<(), RuntimeError> {
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
        let mut first_error = None;
        for (turn_id, active) in active_turns {
            if let Err(error) = self.finalize_shutdown_turn(&active.task_id, &turn_id) {
                first_error.get_or_insert(error);
            }
        }
        if let Err(error) = self.wait_for_active_turns_to_exit() {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Closes OpenAIDE's live handle without deleting the Agent-owned session.
    pub(crate) fn close_session_for_task_history_reset(
        &self,
        session: &AgentSessionKey,
    ) -> Result<(), RuntimeError> {
        self.agent.close_session(session)
    }

    #[cfg(test)]
    pub(crate) fn active_turns(&self) -> HashSet<(String, String)> {
        self.active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .iter()
            .map(|(turn_id, active)| (active.task_id.clone(), turn_id.clone()))
            .collect()
    }

    pub(crate) fn server_requests(&self) -> ServerRequestRuntime {
        self.server_requests.clone()
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
            .end_active_work(
                task_id,
                Some(turn_id),
                crate::tasks::transitions::ActiveWorkEnd::Shutdown,
            )
            .map(|_| ())
    }

    fn transitions(&self) -> TaskTransitions {
        TaskTransitions::new(self.mutations.clone(), self.server_requests.clone())
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
