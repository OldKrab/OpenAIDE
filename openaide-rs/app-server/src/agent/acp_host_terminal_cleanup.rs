use agent_client_protocol::schema::{
    KillTerminalRequest, ReleaseTerminalRequest, WaitForTerminalExitRequest,
};

use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

pub(super) fn kill_host_terminal(
    host_bridge: &HostBridge,
    session_id: &str,
    terminal_id: &str,
) -> bool {
    match request(
        host_bridge,
        "terminal/kill",
        KillTerminalRequest::new(session_id.to_string(), terminal_id.to_string()),
    ) {
        Ok(()) => true,
        Err(error) => {
            log_cleanup_failure(
                "acp_owned_terminal_kill_failed",
                session_id,
                terminal_id,
                &error,
            );
            false
        }
    }
}

pub(super) fn wait_for_host_terminal_exit(
    host_bridge: &HostBridge,
    session_id: &str,
    terminal_id: &str,
) {
    if let Err(error) = request(
        host_bridge,
        "terminal/wait_for_exit",
        WaitForTerminalExitRequest::new(session_id.to_string(), terminal_id.to_string()),
    ) {
        log_cleanup_failure(
            "acp_owned_terminal_exit_wait_failed",
            session_id,
            terminal_id,
            &error,
        );
    }
}

pub(super) fn release_host_terminal(
    host_bridge: &HostBridge,
    session_id: &str,
    terminal_id: &str,
) -> Result<(), RuntimeError> {
    request(
        host_bridge,
        "terminal/release",
        ReleaseTerminalRequest::new(session_id.to_string(), terminal_id.to_string()),
    )
}

fn request(
    host_bridge: &HostBridge,
    method: &'static str,
    request: impl serde::Serialize,
) -> Result<(), RuntimeError> {
    let params =
        serde_json::to_value(request).map_err(|error| RuntimeError::Internal(error.to_string()))?;
    host_bridge.request(method, Some(params)).map(|_| ())
}

fn log_cleanup_failure(
    event: &'static str,
    session_id: &str,
    terminal_id: &str,
    error: &RuntimeError,
) {
    logging::warn(
        event,
        serde_json::json!({
            "sessionId": session_id,
            "terminalId": terminal_id,
            "error": error.to_string(),
        }),
    );
}
