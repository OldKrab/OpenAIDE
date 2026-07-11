use super::*;
use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwnerId};
use crate::agent::events::{AgentEvent, AgentPermissionOutcome, AgentPermissionRequest};
use crate::agent::{AgentPrompt, TurnCancellation};
use std::thread;

fn terminal_owner() -> AcpTerminalOwner {
    let registry = AcpHostTerminalRegistry::new(crate::protocol::host::HostBridge::disabled());
    let owner_id = AcpTerminalOwnerId::next();
    registry.begin_open(owner_id);
    registry.owner(owner_id)
}

struct NoopEventSink;

impl AgentEventSink for NoopEventSink {
    fn emit(&self, _event: AgentEvent) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn request_permission(
        &self,
        _request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        Ok(AgentPermissionOutcome::Cancelled)
    }
}

#[test]
fn worker_stopped_error_uses_authentication_failure() {
    let terminal_error = Arc::new(Mutex::new(None));
    record_terminal_error(
        &terminal_error,
        &RuntimeError::Internal(
            "ACP error: Authentication required: { \"data\": null }".to_string(),
        ),
    );

    let error = worker_stopped_error(&terminal_error);

    assert_eq!(
        error.to_string(),
        "runtime not ready: Authentication required. Open Settings and authenticate this Agent before starting a Task."
    );
}

#[test]
fn worker_stopped_error_keeps_generic_fallback_without_terminal_error() {
    let terminal_error = Arc::new(Mutex::new(None));

    let error = worker_stopped_error(&terminal_error);

    assert_eq!(
        error.to_string(),
        "runtime not ready: ACP session worker stopped"
    );
}

#[test]
fn prompt_returns_terminal_error_while_worker_reply_is_pending() {
    let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
    let (config_tx, _config_rx) = tokio_mpsc::unbounded_channel();
    let (cancel_tx, _cancel_rx) = tokio_mpsc::unbounded_channel();
    let (close_tx, _close_rx) = tokio_mpsc::unbounded_channel();
    let terminal_error = Arc::new(Mutex::new(None));
    let client = AcpSessionClient::new(
        command_tx,
        config_tx,
        cancel_tx,
        close_tx,
        terminal_error.clone(),
        terminal_owner(),
    );
    let error_writer = terminal_error.clone();

    thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        *error_writer
            .lock()
            .expect("ACP terminal error lock poisoned") =
            Some("Process exited with status 7".to_string());
    });

    let error = client
        .prompt(
            AgentPrompt {
                task_id: "task_terminal_error".to_string(),
                session_id: "session_terminal_error".to_string(),
                text: "hello".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(NoopEventSink),
        )
        .expect_err("terminal error should stop pending prompt")
        .to_string();

    assert_eq!(error, "runtime not ready: Process exited with status 7");
}
