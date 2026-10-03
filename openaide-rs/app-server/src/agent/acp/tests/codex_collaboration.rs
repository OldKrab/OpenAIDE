use super::*;
use crate::agent::TurnCancellation;

/// Codex ACP 2.x reports collaboration through the Tool title and typed input,
/// without the older `_meta.codex.collaboration` extension.
fn modern_wait() -> SessionUpdate {
    serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "modern-wait",
        "title": "wait",
        "kind": "other",
        "status": "in_progress",
        "rawInput": {
            "prompt": "private delegated instruction",
            "senderThreadId": "parent-thread",
            "receiverThreadIds": [],
            "agentsStates": {},
            "model": null,
            "reasoningEffort": null,
            "status": "inProgress"
        }
    }))
    .unwrap()
}

fn completed_wait() -> SessionUpdate {
    serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "modern-wait",
        "status": "completed"
    }))
    .unwrap()
}

#[test]
fn modern_codex_wait_is_sanitized_live_and_preserved_by_partial_updates() {
    let capture = Arc::new(CapturingEventSink::default());
    let projection = LivePromptProjection::new("codex", capture.clone(), TurnCancellation::new());
    projection.emit(modern_wait()).unwrap();
    projection.emit(completed_wait()).unwrap();

    let events = capture.events();
    assert_eq!(events.len(), 2);
    for (event, status) in events.iter().zip([
        AgentToolCallStatus::InProgress,
        AgentToolCallStatus::Completed,
    ]) {
        let AgentEvent::ToolCall(tool) = event else {
            panic!("expected collaboration Tool summary");
        };
        assert_eq!(tool.tool_call_id, "modern-wait");
        assert_eq!(tool.title, "Wait for subagents");
        assert_eq!(tool.kind, "collaboration");
        assert_eq!(tool.status, status);
        assert!(tool.input_summary.is_none());
        assert!(tool.output_preview.is_none());
        assert!(tool.details.is_none());
    }
}

#[test]
fn modern_codex_wait_replay_retains_one_completed_collaboration_row() {
    let messages = ReplayProjection::for_agent("codex", "parent-session")
        .project(vec![modern_wait(), completed_wait()]);
    assert!(matches!(
        messages.as_slice(),
        [NormalizedMessage::Activity {
            title,
            status: ActivityStatus::Completed,
            steps,
            ..
        }] if title == "Wait for subagents" && matches!(
            steps.as_slice(),
            [crate::protocol::model::ActivityStep::Tool {
                name,
                status: ActivityStatus::Completed,
                input_summary: None,
                output_preview: None,
                details: None,
                ..
            }] if name == "collaboration"
        )
    ));
}

#[test]
fn modern_collaboration_shape_does_not_reinterpret_other_agents() {
    let capture = Arc::new(CapturingEventSink::default());
    let projection =
        LivePromptProjection::new("other-agent", capture.clone(), TurnCancellation::new());
    projection.emit(modern_wait()).unwrap();
    assert!(matches!(
        capture.events().as_slice(),
        [AgentEvent::ToolCall(tool)] if tool.title == "wait" && tool.kind != "collaboration"
    ));
}

#[test]
fn a_wait_title_without_collaboration_input_remains_a_generic_tool() {
    let capture = Arc::new(CapturingEventSink::default());
    let projection = LivePromptProjection::new("codex", capture.clone(), TurnCancellation::new());
    projection
        .emit(SessionUpdate::ToolCall(
            ToolCall::new("ordinary-wait", "wait")
                .raw_input(serde_json::json!({ "timeoutMs": 100 })),
        ))
        .unwrap();
    assert!(matches!(
        capture.events().as_slice(),
        [AgentEvent::ToolCall(tool)] if tool.title == "wait" && tool.kind != "collaboration"
    ));
}

#[test]
fn malformed_codex_collaboration_remains_generic_and_logs_metadata_only() {
    let diagnostics = crate::logging::capture_test_logs();
    let capture = Arc::new(CapturingEventSink::default());
    let projection = LivePromptProjection::new("codex", capture.clone(), TurnCancellation::new());
    projection
        .emit(
            serde_json::from_value(serde_json::json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "malformed-collaboration",
                "title": "wait",
                "status": "completed",
                "rawInput": {
                    "senderThreadId": "parent-thread",
                    "receiverThreadIds": "private receiver value",
                    "agentsStates": {},
                    "prompt": "private delegated instruction"
                }
            }))
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        capture.events().as_slice(),
        [AgentEvent::ToolCall(tool)] if tool.kind != "collaboration"
    ));
    let entries = diagnostics.snapshot();
    let warning = entries
        .iter()
        .find(|entry| {
            entry["event"] == "acp_codex_collaboration_projection_fallback"
                && entry["fields"]["tool_call_id"] == "malformed-collaboration"
        })
        .expect("invalid collaboration shape must be diagnosable");
    assert_eq!(
        warning["fields"]["reason_code"],
        "invalid_collaboration_input"
    );
    let safe = warning.to_string();
    assert!(!safe.contains("private receiver value"));
    assert!(!safe.contains("private delegated instruction"));
}
