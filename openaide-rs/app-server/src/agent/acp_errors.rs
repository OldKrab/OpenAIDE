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

#[cfg(test)]
mod tests;
