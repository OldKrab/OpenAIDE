use crate::agent::acp_schema::{AuthMethod, InitializeResponse, ProtocolVersion};

use crate::protocol::errors::RuntimeError;

pub(super) fn initialize_supports_session_close(initialize: &InitializeResponse) -> bool {
    initialize
        .agent_capabilities
        .session_capabilities
        .close
        .is_some()
}

pub(super) fn initialize_supports_session_delete(initialize: &InitializeResponse) -> bool {
    initialize
        .agent_capabilities
        .session_capabilities
        .delete
        .is_some()
}

pub(super) fn validate_initialize_protocol(
    initialize: &InitializeResponse,
) -> Result<(), RuntimeError> {
    if initialize.protocol_version == ProtocolVersion::V1 {
        Ok(())
    } else {
        Err(RuntimeError::Unsupported(format!(
            "unsupported ACP protocol version {}",
            initialize.protocol_version
        )))
    }
}

pub(super) fn validate_auth_method(
    initialize: &InitializeResponse,
    method_id: &str,
) -> Result<(), RuntimeError> {
    let method = initialize
        .auth_methods
        .iter()
        .find(|method| method.id().0.as_ref() == method_id)
        .ok_or_else(|| RuntimeError::InvalidParams("method_id".to_string()))?;
    match method {
        AuthMethod::Agent(_) | AuthMethod::EnvVar(_) | AuthMethod::Terminal(_) => Ok(()),
        _ => Err(RuntimeError::CapabilityMissing(format!(
            "auth method {} is not supported",
            auth_method_kind(method)
        ))),
    }
}

pub(super) fn auth_method_kind(method: &AuthMethod) -> String {
    serde_json::to_value(method)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(|kind| kind.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "agent".to_string())
}

#[cfg(test)]
pub(super) fn validate_session_list_capability(
    initialize: &InitializeResponse,
) -> Result<(), RuntimeError> {
    if initialize
        .agent_capabilities
        .session_capabilities
        .list
        .is_some()
    {
        Ok(())
    } else {
        Err(RuntimeError::CapabilityMissing(
            "agent session list is not available".to_string(),
        ))
    }
}

pub(super) fn validate_load_session_capability(
    initialize: &InitializeResponse,
) -> Result<(), RuntimeError> {
    if initialize.agent_capabilities.load_session {
        Ok(())
    } else {
        Err(RuntimeError::CapabilityMissing(
            "agent session load is not available".to_string(),
        ))
    }
}

pub(super) fn validate_resume_session_capability(
    initialize: &InitializeResponse,
) -> Result<(), RuntimeError> {
    if initialize
        .agent_capabilities
        .session_capabilities
        .resume
        .is_some()
    {
        Ok(())
    } else {
        Err(RuntimeError::CapabilityMissing(
            "agent session resume is not available".to_string(),
        ))
    }
}
