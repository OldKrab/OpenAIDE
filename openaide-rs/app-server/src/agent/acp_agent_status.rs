use agent_client_protocol::schema::{AuthMethod, InitializeResponse};

use crate::agent::acp_session_lifecycle::auth_method_kind;
use crate::protocol::model::{
    AgentAuthMethodSummary, AgentProbeCapabilities, AgentProbeResult, AgentProbeStatus,
};

pub(super) fn agent_probe_result_from_initialize(
    agent_id: String,
    initialize: &InitializeResponse,
) -> AgentProbeResult {
    let implementation_name = initialize
        .agent_info
        .as_ref()
        .map(|info| info.title.clone().unwrap_or_else(|| info.name.clone()));
    let implementation_version = initialize
        .agent_info
        .as_ref()
        .map(|info| info.version.clone());

    AgentProbeResult {
        agent_id,
        status: AgentProbeStatus::Ready,
        protocol_version: initialize.protocol_version.to_string(),
        implementation_name,
        implementation_version,
        capabilities: capability_labels(initialize),
        typed_capabilities: typed_capabilities(initialize),
        auth_methods: initialize
            .auth_methods
            .iter()
            .map(auth_method_summary)
            .collect(),
    }
}

fn typed_capabilities(initialize: &InitializeResponse) -> AgentProbeCapabilities {
    let caps = &initialize.agent_capabilities;
    AgentProbeCapabilities {
        resume_sessions: caps.session_capabilities.resume.is_some(),
        delete_sessions: caps.session_capabilities.delete.is_some(),
    }
}

fn capability_labels(initialize: &InitializeResponse) -> Vec<String> {
    let caps = &initialize.agent_capabilities;
    let mut labels = vec!["Basic sessions".to_string()];
    if caps.load_session {
        labels.push("Load sessions".to_string());
    }
    if caps.prompt_capabilities.image {
        labels.push("Image prompts".to_string());
    }
    if caps.prompt_capabilities.audio {
        labels.push("Audio prompts".to_string());
    }
    if caps.prompt_capabilities.embedded_context {
        labels.push("Embedded context".to_string());
    }
    if caps.mcp_capabilities.http {
        labels.push("HTTP MCP".to_string());
    }
    if caps.mcp_capabilities.sse {
        labels.push("SSE MCP".to_string());
    }
    if caps.session_capabilities.list.is_some() {
        labels.push("List sessions".to_string());
    }
    if caps.session_capabilities.resume.is_some() {
        labels.push("Resume sessions".to_string());
    }
    if caps.session_capabilities.close.is_some() {
        labels.push("Close sessions".to_string());
    }
    if caps.session_capabilities.delete.is_some() {
        labels.push("Delete sessions".to_string());
    }
    labels
}

fn auth_method_summary(method: &AuthMethod) -> AgentAuthMethodSummary {
    AgentAuthMethodSummary {
        id: method.id().0.as_ref().to_string(),
        label: method.name().to_string(),
        kind: auth_method_kind(method),
        description: method.description().map(ToString::to_string),
    }
}
