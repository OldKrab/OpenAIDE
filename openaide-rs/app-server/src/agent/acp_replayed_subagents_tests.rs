use super::*;
use crate::agent::events::AgentNativeSubagentState;

fn notification(session_id: &str, update: serde_json::Value) -> SessionNotification {
    serde_json::from_value(serde_json::json!({ "sessionId": session_id, "update": update }))
        .unwrap()
}

fn project(traffic: Vec<SessionNotification>) -> Vec<AgentReplayedSubagentEvent> {
    project_replayed_subagents("codex", None, "root", traffic)
}

#[test]
fn a_replayed_child_becomes_ordered_subagent_facts() {
    let events = project(vec![
        notification(
            "root",
            serde_json::json!({
                "sessionUpdate": "subagent_update",
                "sessionId": "child",
                "title": "Reviewer",
                "state": { "state": "running" },
            }),
        ),
        notification(
            "child",
            serde_json::json!({
                "sessionUpdate": "session_message",
                "messageId": "prompt",
                "senderSessionId": "root",
                "recipientSessionId": "child",
                "content": [{ "type": "text", "text": "Review it" }],
            }),
        ),
        notification(
            "child",
            serde_json::json!({
                "sessionUpdate": "agent_message_chunk",
                "messageId": "reply",
                "content": { "type": "text", "text": "Done" },
            }),
        ),
        notification(
            "root",
            serde_json::json!({
                "sessionUpdate": "subagent_update",
                "sessionId": "child",
                "state": { "state": "idle", "stopReason": "end_turn" },
            }),
        ),
    ]);

    assert!(matches!(
        events.as_slice(),
        [
            AgentReplayedSubagentEvent::Spawned(spawned),
            AgentReplayedSubagentEvent::State(running),
            AgentReplayedSubagentEvent::Update {
                event: AgentEvent::UserMessageChunk { .. },
                ..
            },
            AgentReplayedSubagentEvent::Update {
                event: AgentEvent::MessageChunk { .. },
                ..
            },
            AgentReplayedSubagentEvent::State(done),
        ] if spawned.name == "Reviewer"
            && running.state == AgentNativeSubagentState::Running
            && done.state == AgentNativeSubagentState::Completed
    ));
}

#[test]
fn a_rejected_update_drops_only_itself() {
    let announcement = |parent: &str| {
        notification(
            parent,
            serde_json::json!({ "sessionUpdate": "subagent_update", "sessionId": "child" }),
        )
    };
    // The second parent is unknown, as a live router would refuse it.
    let events = project(vec![announcement("root"), announcement("elsewhere")]);
    assert!(matches!(
        events.as_slice(),
        [AgentReplayedSubagentEvent::Spawned(_)]
    ));
}

#[test]
fn a_replay_without_children_projects_nothing() {
    assert!(project(Vec::new()).is_empty());
}
