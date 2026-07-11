use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::registry::{
    AgentDefinition, AgentLaunch, AgentSourceKind, CODEX_AGENT_ID, CODEX_AGENT_LABEL,
    OPENCODE_AGENT_ID, OPENCODE_AGENT_LABEL,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltInAgentMetadata {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) description: &'static str,
}

const BUILT_IN_DESCRIPTION: &str =
    "Built-in ACP Agent. Configuration Options are discovered before task start.";

pub(crate) const BUILT_IN_AGENT_METADATA: [BuiltInAgentMetadata; 2] = [
    BuiltInAgentMetadata {
        id: CODEX_AGENT_ID,
        label: CODEX_AGENT_LABEL,
        icon: "openai",
        description: BUILT_IN_DESCRIPTION,
    },
    BuiltInAgentMetadata {
        id: OPENCODE_AGENT_ID,
        label: OPENCODE_AGENT_LABEL,
        icon: "opencode",
        description: BUILT_IN_DESCRIPTION,
    },
];

pub(super) fn codex_definition(config: AcpAgentConfig) -> AgentDefinition {
    AgentDefinition::new(
        CODEX_AGENT_ID.to_string(),
        CODEX_AGENT_LABEL.to_string(),
        AgentSourceKind::BuiltIn,
        AgentLaunch::AcpStdio(config),
    )
}

pub(super) fn opencode_definition(config: AcpAgentConfig) -> AgentDefinition {
    AgentDefinition::new(
        OPENCODE_AGENT_ID.to_string(),
        OPENCODE_AGENT_LABEL.to_string(),
        AgentSourceKind::BuiltIn,
        AgentLaunch::AcpStdio(config),
    )
}

pub(super) fn default_definitions() -> [AgentDefinition; 2] {
    [
        codex_definition(AcpAgentConfig::codex()),
        opencode_definition(AcpAgentConfig::opencode()),
    ]
}

pub(super) fn known_built_in_launch(agent_id: &str) -> Option<AcpAgentConfig> {
    match agent_id {
        CODEX_AGENT_ID => Some(AcpAgentConfig::codex()),
        OPENCODE_AGENT_ID => Some(AcpAgentConfig::opencode()),
        _ => None,
    }
}
