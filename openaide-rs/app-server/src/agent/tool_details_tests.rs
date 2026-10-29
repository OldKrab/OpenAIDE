use super::*;
use crate::agent::acp_schema::{ImageContent, Terminal, TextContent};

fn details_of(tool_call: ToolCall) -> ActivityToolDetails {
    let AgentEvent::ToolCall(tool) = tool_call_event(&tool_call) else {
        panic!("expected tool event");
    };
    *tool.details.expect("tool details")
}

#[test]
fn claude_raw_output_repeating_content_is_stored_once() {
    let text = "fn main() {\n    println!(\"repeated in the fenced content\");\n}";
    let details = details_of(
        ToolCall::new("tool-1", "Read main.rs")
            .kind(ToolKind::Read)
            .content(vec![ToolCallContent::from(ContentBlock::Text(
                TextContent::new(format!("```\n{text}\n```")),
            ))])
            .raw_output(Value::String(text.to_string())),
    );
    assert_eq!(details.content.len(), 1);
    assert!(details.output.is_none(), "the text is already in content");

    let data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    let details = details_of(
        ToolCall::new("tool-2", "Read image.png")
            .kind(ToolKind::Read)
            .content(vec![ToolCallContent::from(ContentBlock::Image(
                ImageContent::new(data, "image/png"),
            ))])
            .raw_output(serde_json::json!([{
                "type": "image",
                "source": { "type": "base64", "data": data, "media_type": "image/png" },
            }])),
    );
    assert!(matches!(
        details.content.first(),
        Some(ActivityToolContent::Image { .. })
    ));
    assert!(details.output.is_none(), "the image is already in content");
}

#[test]
fn raw_output_with_anything_beyond_content_is_kept() {
    let fenced = |text: &str| {
        vec![ToolCallContent::from(ContentBlock::Text(TextContent::new(
            format!("```console\n{text}\n```"),
        )))]
    };
    let shown = "a line long enough to count as repeated output text";
    let different = details_of(
        ToolCall::new("tool-1", "Run")
            .kind(ToolKind::Execute)
            .content(fenced(shown))
            .raw_output(serde_json::json!([
                shown,
                "another line that the content never showed"
            ])),
    );
    assert!(
        different.output.is_some(),
        "a string missing from content is kept"
    );

    let structured = details_of(
        ToolCall::new("tool-2", "Run")
            .kind(ToolKind::Execute)
            .content(fenced(shown))
            .raw_output(serde_json::json!({ "stdout": shown, "exit_code": 0 })),
    );
    assert_eq!(
        structured.output.and_then(|output| output.exit_code),
        Some(0),
        "an object carries fields beyond the text"
    );
}

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
