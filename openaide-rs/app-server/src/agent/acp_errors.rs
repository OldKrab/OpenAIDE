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
    if message.to_ascii_lowercase().contains("not found")
        || message.to_ascii_lowercase().contains("does not exist")
    {
        return RuntimeError::TaskNotFound(message);
    }
    acp_error(error)
}

#[cfg(test)]
#[path = "acp_errors_tests.rs"]
mod tests;
