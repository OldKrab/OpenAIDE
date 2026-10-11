use crate::protocol::errors::RuntimeError;

pub(super) fn acp_error(error: impl std::fmt::Display) -> RuntimeError {
    let message = error.to_string();
    if message.contains("Authentication required") {
        return RuntimeError::AuthRequired(
            "Authentication required. Open Settings and authenticate this Agent before starting a Task."
                .to_string(),
        );
    }
    RuntimeError::Internal(format!("ACP error: {message}"))
}

/// Preserve the ACP error code; agents are not required to use a particular message.
pub(super) fn acp_request_error(error: &agent_client_protocol::Error) -> RuntimeError {
    if error.code == crate::agent::acp_schema::ErrorCode::AuthRequired {
        return RuntimeError::AuthRequired(
            "Authentication required. Open Settings and authenticate this Agent before starting a Task."
                .to_string(),
        );
    }
    let message = error.to_string();
    let normalized = message.to_ascii_lowercase();
    // Generic missing files/configuration and an unloaded thread are not evidence
    // that Agent-owned conversation history disappeared.
    if error.code == crate::agent::acp_schema::ErrorCode::ResourceNotFound
        || normalized.contains("no rollout found for thread id")
        || normalized.contains("session not found")
        || normalized.contains("session does not exist")
    {
        return RuntimeError::NativeSessionMissing(message);
    }
    // Preserve empty Prepared-session recovery without authorizing history removal.
    if normalized.contains("thread not loaded:") {
        return RuntimeError::TaskNotFound(message);
    }
    // Codex allows only one writable attachment per thread. Treat this provider-specific
    // rejection as product contention rather than exposing its raw internal error.
    if normalized.contains("already has an active writer") {
        return RuntimeError::Conflict("Native Session is currently in use elsewhere".to_string());
    }
    acp_error(error)
}

/// Classifies a failed `session/prompt`. Sign-in is recognised by the ACP code, whatever
/// wording the Agent chose. Session-missing evidence is deliberately not read here: a
/// failed turn never authorises removing history.
pub(super) fn acp_prompt_error(error: &agent_client_protocol::Error) -> RuntimeError {
    if error.code == crate::agent::acp_schema::ErrorCode::AuthRequired {
        return acp_error("Authentication required");
    }
    match agent_authored_message(error) {
        Some(message) => RuntimeError::AgentReported(message),
        None => acp_error(error),
    }
}

/// Longest Agent failure text carried to Chat; the rest stays in diagnostics.
const AGENT_MESSAGE_LIMIT: usize = 600;

/// The explanation the Agent wrote for the user, if it wrote one. JSON-RPC puts it in
/// `message`; Agents that keep the standard title there put theirs in `data.message`.
fn agent_authored_message(error: &agent_client_protocol::Error) -> Option<String> {
    use agent_client_protocol::Error;
    let standard_titles = [
        Error::parse_error().message,
        Error::invalid_request().message,
        Error::method_not_found().message,
        Error::invalid_params().message,
        Error::internal_error().message,
        Error::request_cancelled().message,
        Error::auth_required().message,
        Error::resource_not_found(None).message,
    ];
    let from_data = error
        .data
        .as_ref()
        .and_then(|data| data.get("message"))
        .and_then(serde_json::Value::as_str);
    let from_message = Some(error.message.as_str()).filter(|message| {
        !standard_titles
            .iter()
            .any(|title| title.eq_ignore_ascii_case(message.trim()))
    });
    let text = from_data
        .or(from_message)
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|text| !text.is_empty())?;
    Some(match text.char_indices().nth(AGENT_MESSAGE_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    })
}

#[cfg(test)]
#[path = "acp_errors_tests.rs"]
mod tests;
