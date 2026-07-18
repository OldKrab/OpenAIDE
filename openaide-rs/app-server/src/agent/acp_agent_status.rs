use crate::agent::acp_schema::{AuthMethod, InitializeResponse};

use crate::agent::acp_session_lifecycle::auth_method_kind;
use crate::protocol::model::{
    AgentAuthMethodSummary, AgentAuthVariableSummary, AgentProbeCapabilities, AgentProbeResult,
    AgentProbeStatus,
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
        logout_supported: initialize.agent_capabilities.auth.logout.is_some(),
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
    let (variables, link, terminal_args, terminal_env) = match method {
        AuthMethod::Agent(_) => (Vec::new(), None, Vec::new(), Default::default()),
        AuthMethod::EnvVar(method) => (
            method
                .vars
                .iter()
                .map(|variable| AgentAuthVariableSummary {
                    name: variable.name.clone(),
                    label: variable.label.clone(),
                    secret: variable.secret,
                    optional: variable.optional,
                })
                .collect(),
            method.link.clone(),
            Vec::new(),
            Default::default(),
        ),
        AuthMethod::Terminal(method) => (Vec::new(), None, method.args.clone(), method.env.clone()),
        _ => (Vec::new(), None, Vec::new(), Default::default()),
    };
    AgentAuthMethodSummary {
        id: method.id().0.as_ref().to_string(),
        label: method.name().to_string(),
        kind: auth_method_kind(method),
        description: method.description().map(ToString::to_string),
        variables,
        link,
        terminal_args,
        terminal_env,
    }
}
