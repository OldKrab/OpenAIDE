use super::command_not_found_error;
use crate::protocol::errors::RuntimeError;

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
