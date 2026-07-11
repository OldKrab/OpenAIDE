use crate::protocol::errors::RuntimeError;

pub(super) fn startup_error_message(error: &RuntimeError) -> String {
    match error {
        RuntimeError::Internal(message)
        | RuntimeError::NotReady(message)
        | RuntimeError::AuthRequired(message)
        | RuntimeError::SetupRequired(message)
        | RuntimeError::Unsupported(message) => message.clone(),
        _ => error.to_string(),
    }
}

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

#[cfg(test)]
mod tests;
