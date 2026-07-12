use agent_client_protocol::schema::{AuthMethod, InitializeResponse, ProtocolVersion};

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
    let kind = auth_method_kind(method);
    if kind == "agent" {
        Ok(())
    } else {
        Err(RuntimeError::CapabilityMissing(format!(
            "auth method {kind} is not available yet"
        )))
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

pub(super) fn auth_method_for_session_retry(
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
) -> Option<String> {
    if let Some(method_id) = preferred_auth_method_id {
        if validate_auth_method(initialize, method_id).is_ok() {
            return Some(method_id.to_string());
        }
    }
    let mut agent_methods = initialize
        .auth_methods
        .iter()
        .filter(|method| auth_method_kind(method) == "agent");
    let method = agent_methods.next()?;
    if agent_methods.next().is_some() {
        return None;
    }
    Some(method.id().0.as_ref().to_string())
}
