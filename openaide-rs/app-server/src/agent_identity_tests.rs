use openaide_app_server_protocol::ids::AgentId;

use super::{
    default_agent_id, normalized_existing_custom_agent_id, normalized_icon, normalized_label,
    valid_env_name,
};
use crate::agent::registry::{AgentDefinitionSummary, AgentSourceKind};

#[test]
fn existing_custom_agent_id_accepts_custom_valid_ids() {
    let agent_id = normalized_existing_custom_agent_id(AgentId::from("custom.local-agent_1"))
        .expect("valid custom id");

    assert_eq!(agent_id.as_str(), "custom.local-agent_1");
}

#[test]
fn existing_custom_agent_id_rejects_builtin_or_invalid_ids() {
    assert_eq!(
        normalized_existing_custom_agent_id(AgentId::from("codex"))
            .unwrap_err()
            .field(),
        "agentId"
    );
    assert_eq!(
        normalized_existing_custom_agent_id(AgentId::from("custom.bad id"))
            .unwrap_err()
            .field(),
        "agentId"
    );
}

#[test]
fn labels_and_icons_are_trimmed_and_capped() {
    assert_eq!(
        normalized_label("  Local Agent  ".to_string()).unwrap(),
        "Local Agent"
    );
    assert_eq!(
        normalized_label(" ".to_string()).unwrap_err().field(),
        "label"
    );
    assert_eq!(normalized_icon(format!("  {}  ", "x".repeat(48))).len(), 40);
}

#[test]
fn env_names_follow_shell_identifier_shape() {
    assert!(valid_env_name("OPENAIDE_TOKEN"));
    assert!(valid_env_name("_TOKEN1"));
    assert!(!valid_env_name("1TOKEN"));
    assert!(!valid_env_name("BAD-NAME"));
}

#[test]
fn default_agent_prefers_codex_then_first_summary() {
    let summaries = vec![
        summary("custom.local"),
        summary("codex"),
        summary("opencode"),
    ];

    assert_eq!(default_agent_id(&summaries), Some(AgentId::from("codex")));
    assert_eq!(
        default_agent_id(&[summary("custom.local")]),
        Some(AgentId::from("custom.local"))
    );
    assert_eq!(default_agent_id(&[]), None);
}

fn summary(id: &str) -> AgentDefinitionSummary {
    AgentDefinitionSummary {
        id: id.to_string(),
        label: id.to_string(),
        source_kind: AgentSourceKind::Custom,
    }
}
