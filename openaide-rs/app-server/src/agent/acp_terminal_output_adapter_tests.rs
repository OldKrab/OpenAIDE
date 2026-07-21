use super::*;

#[test]
fn typed_delta_debug_output_redacts_terminal_identity_and_bytes() {
    let update: ToolCallUpdate = serde_json::from_value(serde_json::json!({
        "toolCallId": "tool_call_1",
        "_meta": {
            "terminal_output_delta": {
                "terminal_id": "private-terminal-id",
                "data": "private terminal output"
            }
        }
    }))
    .unwrap();

    let append = terminal_append(CODEX_AGENT_ID, &update).unwrap();
    let debug = format!("{append:?}");
    assert!(!debug.contains("private-terminal-id"));
    assert!(!debug.contains("private terminal output"));
    assert!(debug.contains("terminal_id_bytes"));
    assert!(debug.contains("data_bytes"));
}
