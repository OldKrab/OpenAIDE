use super::*;
use crate::agent::acp_schema::Terminal;

#[test]
fn terminal_reference_preserves_details_without_inventing_output_text() {
    let event = tool_call_event(
        &ToolCall::new("tool-1", "Run tests")
            .kind(ToolKind::Execute)
            .content(vec![ToolCallContent::Terminal(Terminal::new("terminal-1"))]),
    );

    let AgentEvent::ToolCall(tool) = event else {
        panic!("expected tool event");
    };
    assert_eq!(tool.output_preview, None);
    assert!(matches!(
        tool.details.as_deref().and_then(|details| details.content.first()),
        Some(ActivityToolContent::Terminal { terminal_id }) if terminal_id == "terminal-1"
    ));
}

#[test]
fn execute_tool_carries_agent_description_beside_the_command() {
    let described = tool_call_event(
        &ToolCall::new("tool-1", "sed -i s/a/b/ notes.txt")
            .kind(ToolKind::Execute)
            .raw_input(serde_json::json!({
                "command": "sed -i s/a/b/ notes.txt",
                "description": "  Rename the marker with token=secret  ",
            })),
    );
    let AgentEvent::ToolCall(described) = described else {
        panic!("expected tool event");
    };
    assert_eq!(
        described.description.as_deref(),
        Some("Rename the marker with token=[redacted]")
    );
    assert_eq!(
        described.input_summary.as_deref(),
        Some("sed -i s/a/b/ notes.txt")
    );

    let undescribed = tool_call_event(
        &ToolCall::new("tool-2", "Shell command")
            .kind(ToolKind::Execute)
            .raw_input(serde_json::json!({ "cmd": "ls" })),
    );
    let AgentEvent::ToolCall(undescribed) = undescribed else {
        panic!("expected tool event");
    };
    assert_eq!(undescribed.description, None);

    // Only execute Tools own a purpose line; other kinds keep their subject title.
    let fetch = tool_call_event(
        &ToolCall::new("tool-3", "Fetch")
            .kind(ToolKind::Fetch)
            .raw_input(serde_json::json!({
                "url": "https://example.com",
                "description": "Read the page",
            })),
    );
    let AgentEvent::ToolCall(fetch) = fetch else {
        panic!("expected tool event");
    };
    assert_eq!(fetch.description, None);
}
