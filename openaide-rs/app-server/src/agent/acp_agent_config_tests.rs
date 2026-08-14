use super::command_not_found_error;
use super::AcpAgentConfig;
use crate::protocol::errors::RuntimeError;

#[test]
fn built_in_codex_uses_the_product_pinned_adapter() {
    let config = AcpAgentConfig::codex_npx_fallback();

    assert_eq!(config.agent_id, "codex");
    assert_eq!(config.command, "npx");
    assert_eq!(config.args, ["-y", "@openaide/codex-acp@1.3.0-openaide.1"]);
}

#[test]
fn missing_codex_npx_is_classified_as_node_js_required() {
    assert!(matches!(
        command_not_found_error("codex", "npx"),
        RuntimeError::NodeJsRequired(_)
    ));
}

#[test]
fn missing_custom_npx_remains_a_generic_setup_failure() {
    assert!(matches!(
        command_not_found_error("custom.local", "npx"),
        RuntimeError::SetupRequired(_)
    ));
}
