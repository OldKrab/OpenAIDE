use super::{command_not_found_error, AcpAgentConfig};
use crate::protocol::errors::RuntimeError;

#[test]
fn codex_npx_fallback_uses_the_release_tested_exact_version() {
    let config = AcpAgentConfig::codex_npx_fallback();

    assert_eq!(config.command, "npx");
    assert_eq!(config.args, ["-y", "@agentclientprotocol/codex-acp@1.1.4"]);
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
