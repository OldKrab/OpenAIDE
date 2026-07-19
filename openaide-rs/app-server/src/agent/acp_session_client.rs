use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_host_terminal_ownership::AcpTerminalOwner;
use crate::agent::{
    AgentEventSink, AgentLoadedSession, AgentPrompt, AgentPromptOutcome, AgentSessionEventSink,
    AgentSessionLoad, TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;

const CANCEL_PROCESS_GRACE: Duration = Duration::from_millis(250);

pub(super) struct AcpSessionProcessHandle {
    pub(super) shutdown: tokio::sync::watch::Sender<bool>,
    pub(super) keepalive: Option<crate::agent::acp_agent_process_pool::AcpAgentProcessKeepalive>,
    pub(super) agent_id: String,
    pub(super) session_id: String,
}

#[derive(Clone)]
pub(super) struct AcpSessionClient {
    command_tx: tokio_mpsc::UnboundedSender<AcpSessionCommand>,
    config_tx: tokio_mpsc::UnboundedSender<AcpSessionConfigCommand>,
    cancel_tx: tokio_mpsc::UnboundedSender<()>,
    close_tx: tokio_mpsc::UnboundedSender<mpsc::Sender<Result<(), RuntimeError>>>,
    terminal_error: Arc<Mutex<Option<String>>>,
    terminal_owner: AcpTerminalOwner,
    process_shutdown: tokio::sync::watch::Sender<bool>,
    _process_keepalive: Option<crate::agent::acp_agent_process_pool::AcpAgentProcessKeepalive>,
    agent_id: String,
    session_id: String,
    prompt_lifecycle: Arc<PromptLifecycle>,
}

impl AcpSessionClient {
    #[cfg(test)]
    pub(super) fn new(
        command_tx: tokio_mpsc::UnboundedSender<AcpSessionCommand>,
        config_tx: tokio_mpsc::UnboundedSender<AcpSessionConfigCommand>,
        cancel_tx: tokio_mpsc::UnboundedSender<()>,
        close_tx: tokio_mpsc::UnboundedSender<mpsc::Sender<Result<(), RuntimeError>>>,
        terminal_error: Arc<Mutex<Option<String>>>,
        terminal_owner: AcpTerminalOwner,
    ) -> Self {
        let (process_shutdown, _shutdown_rx) = tokio::sync::watch::channel(false);
        Self::new_managed(
            command_tx,
            config_tx,
            cancel_tx,
            close_tx,
            terminal_error,
            terminal_owner,
            AcpSessionProcessHandle {
                shutdown: process_shutdown,
                keepalive: None,
                agent_id: "unknown".to_string(),
                session_id: "unknown".to_string(),
            },
        )
    }

    pub(super) fn new_managed(
        command_tx: tokio_mpsc::UnboundedSender<AcpSessionCommand>,
        config_tx: tokio_mpsc::UnboundedSender<AcpSessionConfigCommand>,
        cancel_tx: tokio_mpsc::UnboundedSender<()>,
        close_tx: tokio_mpsc::UnboundedSender<mpsc::Sender<Result<(), RuntimeError>>>,
        terminal_error: Arc<Mutex<Option<String>>>,
        terminal_owner: AcpTerminalOwner,
        process: AcpSessionProcessHandle,
    ) -> Self {
        Self {
            command_tx,
            config_tx,
            cancel_tx,
            close_tx,
            terminal_error,
            terminal_owner,
            process_shutdown: process.shutdown,
            _process_keepalive: process.keepalive,
            agent_id: process.agent_id,
            session_id: process.session_id,
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
        let _settlement = self.prompt_lifecycle.admit(&request.cancellation)?;
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
        if cancellation.is_cancelled() {
            return Ok(AgentPromptOutcome::Cancelled);
        }
        if self.has_terminal_error() {
            return Err(self.worker_stopped_error());
        }
        // A cancelled prompt still owns the Native Session until its worker observes
        // the Agent's response. Session updates use the independent permanent listener.
        let _settlement = self.prompt_lifecycle.admit(&cancellation)?;
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
            })
            .map_err(|_| self.worker_stopped_error())?;
        loop {
            match done_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Err(_)) if cancellation.is_cancelled() => {
                    return Ok(AgentPromptOutcome::Cancelled);
                }
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if cancellation.is_cancelled() {
                        return Ok(AgentPromptOutcome::Cancelled);
                    }
                    return Err(self.worker_stopped_error());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.has_terminal_error() {
                        if cancellation.is_cancelled() {
                            return Ok(AgentPromptOutcome::Cancelled);
                        }
                        return Err(self.worker_stopped_error());
                    }
                }
            }
        }
    }

    /// Queues a second ACP prompt without joining the primary prompt lifecycle.
    pub(super) fn steer(&self, prompt: AgentPrompt) -> Result<(), RuntimeError> {
        if prompt.cancellation.is_cancelled() {
            return Ok(());
        }
        if self.has_terminal_error() {
            return Err(self.worker_stopped_error());
        }
        self.command_tx
            .send(AcpSessionCommand::Steer { prompt })
            .map_err(|_| self.worker_stopped_error())
    }

    pub(super) fn set_config_option(
        &self,
        agent_id: String,
        config_id: String,
        value: String,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.config_tx
            .send(AcpSessionConfigCommand::SetConfigOption {
                agent_id,
                config_id,
                value,
                reply_tx,
            })
            .map_err(|_| self.worker_stopped_error())?;
        // Some Agents serialize configuration behind an active tool call. Keep
        // the request alive while the Frontend presents that pending state.
        reply_rx
            .recv_timeout(Duration::from_secs(60))
            .map_err(|error| {
                RuntimeError::NotReady(format!("ACP config update timed out: {error}"))
            })?
    }

    pub(super) fn cancel(&self) -> Result<(), RuntimeError> {
        let cancel_result = self
            .cancel_tx
            .send(())
            .map_err(|_| self.worker_stopped_error());
        let cleanup_result = self.terminal_owner.cancel();
        if cancel_result.is_ok() {
            let shutdown = self.process_shutdown.clone();
            let agent_id = self.agent_id.clone();
            let session_id = self.session_id.clone();
            std::thread::spawn(move || {
                std::thread::sleep(CANCEL_PROCESS_GRACE);
                let process_was_active = shutdown.send(true).is_ok();
                crate::logging::info(
                    "acp_cancel_process_group_termination_requested",
                    serde_json::json!({
                        "agent_id": agent_id,
                        "session_id": session_id,
                        "process_was_active": process_was_active,
                    }),
                );
            });
        }
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
    active: Mutex<Option<TurnCancellation>>,
    settled: Condvar,
}

impl PromptLifecycle {
    fn admit(
        self: &Arc<Self>,
        cancellation: &TurnCancellation,
    ) -> Result<PromptSettlementGuard, RuntimeError> {
        let mut active = self.active.lock().expect("ACP prompt lifecycle poisoned");
        loop {
            match active.as_ref() {
                None => {
                    *active = Some(cancellation.clone());
                    return Ok(PromptSettlementGuard {
                        lifecycle: self.clone(),
                    });
                }
                Some(current) if current.is_cancelled() => {
                    active = self
                        .settled
                        .wait(active)
                        .expect("ACP prompt lifecycle poisoned");
                }
                Some(_) => {
                    return Err(RuntimeError::NotReady(
                        "ACP session already has an active prompt".to_string(),
                    ));
                }
            }
        }
    }
}

struct PromptSettlementGuard {
    lifecycle: Arc<PromptLifecycle>,
}

impl Drop for PromptSettlementGuard {
    fn drop(&mut self) {
        self.lifecycle
            .active
            .lock()
            .expect("ACP prompt lifecycle poisoned")
            .take();
        self.lifecycle.settled.notify_all();
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
    },
    Steer {
        prompt: AgentPrompt,
    },
    Delete {
        reply_tx: mpsc::Sender<Result<(), RuntimeError>>,
    },
}

pub(super) enum AcpSessionConfigCommand {
    SetConfigOption {
        agent_id: String,
        config_id: String,
        value: String,
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
