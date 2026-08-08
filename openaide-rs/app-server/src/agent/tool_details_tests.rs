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
