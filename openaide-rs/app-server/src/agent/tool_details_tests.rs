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
fn claude_skill_tool_is_a_skill_activation_named_by_its_input() {
    let claude_meta = |tool_name: &str| {
        serde_json::json!({ "claudeCode": { "toolName": tool_name } })
            .as_object()
            .cloned()
    };
    let skill = tool_call_event(
        &ToolCall::new("tool-1", "Load skill: prototype")
            .kind(ToolKind::Other)
            .raw_input(serde_json::json!({
                "skill": "prototype",
                "args": "Redesign the compaction row",
            }))
            .raw_output(serde_json::json!("Launching skill: prototype"))
            .meta(claude_meta("Skill")),
    );
    let AgentEvent::ToolCall(skill) = skill else {
        panic!("expected tool event");
    };
    assert_eq!(skill.kind, "skill");
    assert_eq!(skill.input_summary.as_deref(), Some("prototype"));

    // The input can stream in after the call is announced; keep the generic
    // row until the skill is named.
    let unnamed = tool_call_event(
        &ToolCall::new("tool-2", "Load skill")
            .kind(ToolKind::Other)
            .raw_input(serde_json::json!({}))
            .meta(claude_meta("Skill")),
    );
    let AgentEvent::ToolCall(unnamed) = unnamed else {
        panic!("expected tool event");
    };
    assert_eq!(unnamed.kind, "other");

    // Another tool with a `skill` argument is not an activation.
    let other = tool_call_event(
        &ToolCall::new("tool-3", "Lookup")
            .kind(ToolKind::Other)
            .raw_input(serde_json::json!({ "skill": "prototype" }))
            .meta(claude_meta("mcp__catalog__lookup")),
    );
    let AgentEvent::ToolCall(other) = other else {
        panic!("expected tool event");
    };
    assert_eq!(other.kind, "other");
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
