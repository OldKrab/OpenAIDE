use crate::protocol::errors::RuntimeError;

pub(super) struct AcpConnectionTerminalDiagnostics {
    pub(super) outcome_kind: &'static str,
    pub(super) exit_code: Option<i32>,
    pub(super) exit_signal: Option<i32>,
    pub(super) active_session_count: usize,
    pub(super) active_prompt_count: usize,
}

/// The ACP SDK owns the child handle, so OpenAIDE receives process status through its bounded
/// transport error. Extract only numeric status metadata; never copy Agent stderr into diagnostics.
pub(super) fn acp_connection_terminal_diagnostics(
    result: &Result<(), RuntimeError>,
    requested_shutdown: bool,
    active_session_count: usize,
    active_prompt_count: usize,
) -> AcpConnectionTerminalDiagnostics {
    let (outcome_kind, exit_code, exit_signal) = if requested_shutdown {
        ("requested_shutdown", None, None)
    } else if let Err(error) = result {
        let message = error.to_string();
        let exit_code = numeric_status_after(&message, "exit status:");
        let exit_signal = numeric_status_after(&message, "signal:");
        if exit_code.is_some() || exit_signal.is_some() {
            ("process_exit", exit_code, exit_signal)
        } else {
            ("transport_error", None, None)
        }
    } else {
        ("unexpected_close", None, None)
    };
    AcpConnectionTerminalDiagnostics {
        outcome_kind,
        exit_code,
        exit_signal,
        active_session_count,
        active_prompt_count,
    }
}

fn numeric_status_after(message: &str, marker: &str) -> Option<i32> {
    let suffix = message.split_once(marker)?.1.trim_start();
    let end = suffix
        .char_indices()
        .find(|(index, character)| *index > 0 && !character.is_ascii_digit())
        .map(|(index, _)| index)
        .unwrap_or(suffix.len());
    suffix[..end].parse().ok()
}
