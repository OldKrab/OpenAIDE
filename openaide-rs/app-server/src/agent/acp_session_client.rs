use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_host_terminal_ownership::AcpTerminalOwner;
use crate::agent::{
    AgentEventSink, AgentLoadedSession, AgentPrompt, AgentPromptOutcome, AgentSessionEventSink,
    AgentSessionLoad, TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ConfigOptionCurrentValue, ConfigOptionsCatalog};

#[derive(Clone)]
pub(super) struct AcpSessionClient {
    command_tx: tokio_mpsc::UnboundedSender<AcpSessionCommand>,
    config_tx: tokio_mpsc::UnboundedSender<AcpSessionConfigCommand>,
    cancel_tx: tokio_mpsc::UnboundedSender<()>,
    close_tx: tokio_mpsc::UnboundedSender<mpsc::Sender<Result<(), RuntimeError>>>,
    terminal_error: Arc<Mutex<Option<String>>>,
    terminal_owner: AcpTerminalOwner,
    prompt_lifecycle: Arc<PromptLifecycle>,
}

impl AcpSessionClient {
    pub(super) fn new(
        command_tx: tokio_mpsc::UnboundedSender<AcpSessionCommand>,
        config_tx: tokio_mpsc::UnboundedSender<AcpSessionConfigCommand>,
        cancel_tx: tokio_mpsc::UnboundedSender<()>,
        close_tx: tokio_mpsc::UnboundedSender<mpsc::Sender<Result<(), RuntimeError>>>,
        terminal_error: Arc<Mutex<Option<String>>>,
        terminal_owner: AcpTerminalOwner,
    ) -> Self {
        Self {
            command_tx,
            config_tx,
            cancel_tx,
            close_tx,
            terminal_error,
            terminal_owner,
            prompt_lifecycle: Arc::default(),
        }
    }

    pub(super) fn set_event_sink(
        &self,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.command_tx
            .send(AcpSessionCommand::SetEventSink { sink })
            .map_err(|_| self.worker_stopped_error())
    }

    /// Replays history through the binding that already owns this Native Session.
    pub(super) fn load_session(
        &self,
        request: AgentSessionLoad,
    ) -> Result<AgentLoadedSession, RuntimeError> {
        if request.cancellation.is_cancelled() {
            return Err(RuntimeError::InvalidParams("session cancelled".to_string()));
        }
        if self.has_terminal_error() {
            return Err(self.worker_stopped_error());
        }
        // Loading replaces the worker's attached ACP session, so it must not overlap
        // a prompt that is using the same binding.
        let Some(_admission) = self.prompt_lifecycle.admit(&request.cancellation)? else {
            return Err(RuntimeError::InvalidParams("session cancelled".to_string()));
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        self.command_tx
            .send(AcpSessionCommand::Load { request, reply_tx })
            .map_err(|_| self.worker_stopped_error())?;
        loop {
            match reply_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(self.worker_stopped_error());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.has_terminal_error() {
                        return Err(self.worker_stopped_error());
                    }
                }
            }
        }
    }

    pub(super) fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<AgentPromptOutcome, RuntimeError> {
        let cancellation = prompt.cancellation.clone();
        let task_id = prompt.task_id.clone();
        let session_id = prompt.session_id.clone();
        if cancellation.is_cancelled() {
            return Ok(AgentPromptOutcome::Cancelled);
        }
        if self.has_terminal_error() {
            return Err(self.worker_stopped_error());
        }
        // A cancelled prompt still owns the Native Session until its worker observes
        // the Agent's response. Session updates use the independent permanent listener.
        crate::logging::info(
            "acp_prompt_lifecycle_admission_started",
            serde_json::json!({
                "task_id": task_id.as_str(),
                "session_id": session_id.as_str(),
            }),
        );
        let Some(admission) = self.prompt_lifecycle.admit(&cancellation)? else {
            return Ok(AgentPromptOutcome::Cancelled);
        };
        crate::logging::info(
            "acp_prompt_lifecycle_admitted",
            serde_json::json!({
                "task_id": task_id.as_str(),
                "session_id": session_id.as_str(),
            }),
        );
        if cancellation.is_cancelled() {
            return Ok(AgentPromptOutcome::Cancelled);
        }
        if self.has_terminal_error() {
            return Err(self.worker_stopped_error());
        }
        self.terminal_owner.activate()?;
        if cancellation.is_cancelled() {
            let _ = self.terminal_owner.cancel();
            return Ok(AgentPromptOutcome::Cancelled);
        }
        let (done_tx, done_rx) = mpsc::channel();
        self.command_tx
            .send(AcpSessionCommand::Prompt {
                prompt,
                sink,
                done_tx,
                request_guard: admission.request,
            })
            .map_err(|_| self.worker_stopped_error())?;
        crate::logging::info(
            "acp_prompt_command_queued",
            serde_json::json!({
                "task_id": task_id.as_str(),
                "session_id": session_id.as_str(),
            }),
        );
        loop {
            match done_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(self.worker_stopped_error());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.has_terminal_error() {
                        return Err(self.worker_stopped_error());
                    }
                }
            }
        }
    }

    /// Queues a second ACP prompt in the current prompt generation.
    pub(super) fn steer(&self, prompt: AgentPrompt) -> Result<(), RuntimeError> {
        if prompt.cancellation.is_cancelled() {
            return Ok(());
        }
        if self.has_terminal_error() {
            return Err(self.worker_stopped_error());
        }
        let request_guard = self.prompt_lifecycle.register_steering_request()?;
        self.command_tx
            .send(AcpSessionCommand::Steer {
                prompt,
                request_guard,
            })
            .map_err(|_| self.worker_stopped_error())
    }

    pub(super) fn set_config_option(
        &self,
        agent_id: String,
        session_id: String,
        config_id: String,
        value: ConfigOptionCurrentValue,
        operation_id: String,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        let started_at = Instant::now();
        let (reply_tx, reply_rx) = mpsc::channel();
        let queued_at = Instant::now();
        crate::logging::info(
            "acp_config_option_command_queued",
            serde_json::json!({
                "session_id": session_id,
                "operation_id": operation_id,
            }),
        );
        self.config_tx
            .send(AcpSessionConfigCommand::SetConfigOption {
                agent_id,
                session_id: session_id.clone(),
                config_id,
                value,
                operation_id: operation_id.clone(),
                queued_at,
                reply_tx,
            })
            .map_err(|_| self.worker_stopped_error())?;
        // Some Agents serialize configuration behind an active tool call. Keep
        // the request alive while the Frontend presents that pending state.
        let result = match reply_rx.recv_timeout(Duration::from_secs(60)) {
            Ok(result) => result,
            Err(error) => {
                crate::logging::warn(
                    "acp_config_option_command_completed",
                    serde_json::json!({
                        "session_id": session_id,
                        "operation_id": operation_id,
                        "total_elapsed_ms": started_at.elapsed().as_millis(),
                        "result_status": "timeout",
                    }),
                );
                return Err(RuntimeError::NotReady(format!(
                    "ACP config update timed out: {error}"
                )));
            }
        };
        crate::logging::info(
            "acp_config_option_command_completed",
            serde_json::json!({
                "session_id": session_id,
                "operation_id": operation_id,
                "total_elapsed_ms": started_at.elapsed().as_millis(),
                "result_status": if result.is_ok() { "ok" } else { "error" },
            }),
        );
        result
    }

    pub(super) fn cancel(&self) -> Result<(), RuntimeError> {
        let cancel_result = self
            .cancel_tx
            .send(())
            .map_err(|_| self.worker_stopped_error());
        let cleanup_result = self.terminal_owner.cancel();
        cancel_result.and(cleanup_result)
    }

    pub(super) fn close(&self) -> Result<(), RuntimeError> {
        let cleanup_result = self.terminal_owner.close();
        let (reply_tx, reply_rx) = mpsc::channel();
        self.close_tx
            .send(reply_tx)
            .map_err(|_| self.worker_stopped_error())?;
        let close_result = reply_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| RuntimeError::NotReady(format!("ACP close timed out: {error}")))?;
        cleanup_result.and(close_result)
    }

    pub(super) fn delete(&self) -> Result<(), RuntimeError> {
        let cleanup_result = self.terminal_owner.close();
        let (reply_tx, reply_rx) = mpsc::channel();
        self.command_tx
            .send(AcpSessionCommand::Delete { reply_tx })
            .map_err(|_| self.worker_stopped_error())?;
        let delete_result = reply_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|error| RuntimeError::NotReady(format!("ACP delete timed out: {error}")))?;
        cleanup_result.and(delete_result)
    }

    fn worker_stopped_error(&self) -> RuntimeError {
        worker_stopped_error(&self.terminal_error)
    }

    /// A process error or a dropped per-session worker makes this handle unusable.
    ///
    /// Individual session workers share an Agent process, so their receiver can disappear
    /// without setting the process-wide terminal error.
    pub(super) fn is_running(&self) -> bool {
        !self.has_terminal_error() && !self.command_tx.is_closed()
    }

    fn has_terminal_error(&self) -> bool {
        self.terminal_error
            .lock()
            .expect("ACP terminal error lock poisoned")
            .is_some()
    }
}

#[derive(Default)]
struct PromptLifecycle {
    active: Mutex<Option<PromptGeneration>>,
    settled: Condvar,
    next_generation_id: AtomicU64,
}

struct PromptGeneration {
    id: u64,
    cancellation: TurnCancellation,
    accepting_steering: bool,
    outstanding_requests: usize,
}

impl PromptLifecycle {
    fn admit(
        self: &Arc<Self>,
        cancellation: &TurnCancellation,
    ) -> Result<Option<PromptAdmission>, RuntimeError> {
        let mut active = self.active.lock().expect("ACP prompt lifecycle poisoned");
        loop {
            match active.as_ref() {
                None => {
                    let generation_id = self.next_generation_id.fetch_add(1, Ordering::Relaxed) + 1;
                    *active = Some(PromptGeneration {
                        id: generation_id,
                        cancellation: cancellation.clone(),
                        accepting_steering: true,
                        outstanding_requests: 1,
                    });
                    return Ok(Some(PromptAdmission {
                        _settlement: PromptSettlementGuard {
                            lifecycle: self.clone(),
                            generation_id,
                        },
                        request: PromptRequestGuard {
                            lifecycle: self.clone(),
                            generation_id,
                        },
                    }));
                }
                Some(current)
                    if !current.accepting_steering || current.cancellation.is_cancelled() =>
                {
                    let (next_active, _) = self
                        .settled
                        .wait_timeout(active, Duration::from_millis(100))
                        .expect("ACP prompt lifecycle poisoned");
                    active = next_active;
                    if cancellation.is_cancelled() {
                        return Ok(None);
                    }
                }
                Some(_) => {
                    return Err(RuntimeError::NotReady(
                        "ACP session already has an active prompt".to_string(),
                    ));
                }
            }
        }
    }

    fn register_steering_request(self: &Arc<Self>) -> Result<PromptRequestGuard, RuntimeError> {
        let mut active = self.active.lock().expect("ACP prompt lifecycle poisoned");
        let Some(generation) = active
            .as_mut()
            .filter(|generation| generation.accepting_steering)
        else {
            return Err(RuntimeError::NotReady(
                "ACP session has no active prompt to steer".to_string(),
            ));
        };
        generation.outstanding_requests += 1;
        Ok(PromptRequestGuard {
            lifecycle: self.clone(),
            generation_id: generation.id,
        })
    }

    fn finish_request(&self, generation_id: u64) {
        let mut active = self.active.lock().expect("ACP prompt lifecycle poisoned");
        let Some(generation) = active
            .as_mut()
            .filter(|generation| generation.id == generation_id)
        else {
            return;
        };
        generation.outstanding_requests = generation
            .outstanding_requests
            .checked_sub(1)
            .expect("ACP prompt request count underflow");
    }
}

struct PromptAdmission {
    _settlement: PromptSettlementGuard,
    request: PromptRequestGuard,
}

struct PromptSettlementGuard {
    lifecycle: Arc<PromptLifecycle>,
    generation_id: u64,
}

impl Drop for PromptSettlementGuard {
    fn drop(&mut self) {
        let mut active = self
            .lifecycle
            .active
            .lock()
            .expect("ACP prompt lifecycle poisoned");
        let Some(generation) = active
            .as_mut()
            .filter(|generation| generation.id == self.generation_id)
        else {
            return;
        };
        generation.accepting_steering = false;
        let outstanding_requests = generation.outstanding_requests;
        active.take();
        self.lifecycle.settled.notify_all();
        crate::logging::info(
            "acp_prompt_generation_retired",
            serde_json::json!({
                "generation_id": self.generation_id,
                "outstanding_requests": outstanding_requests,
            }),
        );
    }
}

/// Tags an ACP response waiter so a late response cannot mutate a newer prompt generation.
pub(super) struct PromptRequestGuard {
    lifecycle: Arc<PromptLifecycle>,
    generation_id: u64,
}

impl Drop for PromptRequestGuard {
    fn drop(&mut self) {
        self.lifecycle.finish_request(self.generation_id);
    }
}

pub(super) enum AcpSessionCommand {
    SetEventSink {
        sink: Arc<dyn AgentSessionEventSink>,
    },
    Load {
        request: AgentSessionLoad,
        reply_tx: mpsc::Sender<Result<AgentLoadedSession, RuntimeError>>,
    },
    Prompt {
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
        done_tx: mpsc::Sender<Result<AgentPromptOutcome, RuntimeError>>,
        request_guard: PromptRequestGuard,
    },
    Steer {
        prompt: AgentPrompt,
        request_guard: PromptRequestGuard,
    },
    Delete {
        reply_tx: mpsc::Sender<Result<(), RuntimeError>>,
    },
}

pub(super) enum AcpSessionConfigCommand {
    SetConfigOption {
        agent_id: String,
        session_id: String,
        config_id: String,
        value: ConfigOptionCurrentValue,
        operation_id: String,
        queued_at: Instant,
        reply_tx: mpsc::Sender<Result<ConfigOptionsCatalog, RuntimeError>>,
    },
}

pub(super) fn record_terminal_error(
    terminal_error: &Arc<Mutex<Option<String>>>,
    error: &RuntimeError,
) {
    *terminal_error
        .lock()
        .expect("ACP terminal error lock poisoned") =
        Some(readable_worker_stopped_message(&error.to_string()));
}

fn worker_stopped_error(terminal_error: &Arc<Mutex<Option<String>>>) -> RuntimeError {
    let message = terminal_error
        .lock()
        .expect("ACP terminal error lock poisoned")
        .clone()
        .unwrap_or_else(|| "Native Session worker stopped".to_string());
    RuntimeError::NotReady(message)
}

fn readable_worker_stopped_message(raw: &str) -> String {
    let message = raw.trim();
    if message.contains("Authentication required") {
        return "Authentication required. Open Settings and authenticate this Agent before starting a Task.".to_string();
    }
    if message.contains("Agent command not found") {
        return strip_runtime_error_prefixes(message).to_string();
    }
    strip_runtime_error_prefixes(message).to_string()
}

fn strip_runtime_error_prefixes(mut message: &str) -> &str {
    for prefix in [
        "internal error: ACP error: ",
        "internal error: ",
        "runtime not ready: ",
    ] {
        if let Some(stripped) = message.strip_prefix(prefix) {
            message = stripped;
        }
    }
    message
}

#[cfg(test)]
#[path = "acp_session_client_tests.rs"]
mod tests;
