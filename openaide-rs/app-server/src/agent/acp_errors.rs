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
    if normalized.contains("not found")
        || normalized.contains("does not exist")
        // Codex can create an empty session identity before its first durable rollout.
        // After a restart, that identity is explicitly reported as missing this way.
        || normalized.contains("no rollout found for thread id")
    {
        return RuntimeError::TaskNotFound(message);
    }
    // Codex allows only one writable attachment per thread. Treat this provider-specific
    // rejection as product contention rather than exposing its raw internal error.
    if normalized.contains("already has an active writer") {
        return RuntimeError::Conflict("Native Session is currently in use elsewhere".to_string());
    }
    acp_error(error)
}

#[cfg(test)]
#[path = "acp_errors_tests.rs"]
mod tests;
