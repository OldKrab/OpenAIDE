use openaide_app_server_protocol::ids::AgentId;

use super::{
    normalized_existing_custom_agent_id, normalized_icon, normalized_label, valid_env_name,
};

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
