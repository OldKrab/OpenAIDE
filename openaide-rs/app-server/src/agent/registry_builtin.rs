use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::registry::{
    AgentDefinition, AgentLaunch, AgentSourceKind, CLAUDE_CODE_AGENT_ID, CLAUDE_CODE_AGENT_LABEL,
    CODEX_AGENT_ID, CODEX_AGENT_LABEL, OPENCODE_AGENT_ID, OPENCODE_AGENT_LABEL,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltInAgentMetadata {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) description: &'static str,
}

pub(crate) const BUILT_IN_AGENT_METADATA: [BuiltInAgentMetadata; 3] = [
    BuiltInAgentMetadata {
        id: CODEX_AGENT_ID,
        label: CODEX_AGENT_LABEL,
        icon: "openai",
        description: "OpenAI coding agent.",
    },
    BuiltInAgentMetadata {
        id: OPENCODE_AGENT_ID,
        label: OPENCODE_AGENT_LABEL,
        icon: "opencode",
        description: "Open-source coding agent.",
    },
    BuiltInAgentMetadata {
        id: CLAUDE_CODE_AGENT_ID,
        label: CLAUDE_CODE_AGENT_LABEL,
        icon: "sparkles",
        description: "Anthropic coding agent.",
    },
];

pub(super) fn codex_definition(config: AcpAgentConfig) -> AgentDefinition {
    AgentDefinition::new(
        CODEX_AGENT_ID.to_string(),
        CODEX_AGENT_LABEL.to_string(),
        built_in_icon(CODEX_AGENT_ID).to_string(),
        AgentSourceKind::BuiltIn,
        AgentLaunch::AcpStdio(config),
    )
}

pub(super) fn opencode_definition(config: AcpAgentConfig) -> AgentDefinition {
    AgentDefinition::new(
        OPENCODE_AGENT_ID.to_string(),
        OPENCODE_AGENT_LABEL.to_string(),
        built_in_icon(OPENCODE_AGENT_ID).to_string(),
        AgentSourceKind::BuiltIn,
        AgentLaunch::AcpStdio(config),
    )
}

/// Display icon for a Built-in Agent. Empty icon fields in catalog overlays fall
/// back to this so persisted records from earlier schemas keep their identity.
pub(super) fn built_in_icon(agent_id: &str) -> &'static str {
    BUILT_IN_AGENT_METADATA
        .iter()
        .find(|metadata| metadata.id == agent_id)
        .map(|metadata| metadata.icon)
        .unwrap_or("bot")
}

pub(super) fn default_definitions() -> [AgentDefinition; 3] {
    [
        codex_definition(AcpAgentConfig::codex()),
        opencode_definition(AcpAgentConfig::opencode()),
        AgentDefinition::new(
            CLAUDE_CODE_AGENT_ID.to_string(),
            CLAUDE_CODE_AGENT_LABEL.to_string(),
            built_in_icon(CLAUDE_CODE_AGENT_ID).to_string(),
            AgentSourceKind::BuiltIn,
            AgentLaunch::AcpStdio(AcpAgentConfig::claude_code()),
        ),
    ]
}

pub(super) fn known_built_in_launch(agent_id: &str) -> Option<AcpAgentConfig> {
    match agent_id {
        CODEX_AGENT_ID => Some(AcpAgentConfig::codex()),
        OPENCODE_AGENT_ID => Some(AcpAgentConfig::opencode()),
        CLAUDE_CODE_AGENT_ID => Some(AcpAgentConfig::claude_code()),
        _ => None,
    }
}

pub(super) fn is_built_in_id(agent_id: &str) -> bool {
    BUILT_IN_AGENT_METADATA
        .iter()
        .any(|metadata| metadata.id == agent_id)
}
