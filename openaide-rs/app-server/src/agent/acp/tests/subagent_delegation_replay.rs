//! Parent delegation rows of the released ACP Subagents form: `session/load`
//! replay builds the same rows as the live router.

use super::*;
use crate::agent::acp_native_subagents::{AcpNativeSubagentRouter, RoutedSessionNotification};
use crate::agent::normalizer::normalize_event;

/// The root updates of one delegated run, as the Claude adapter sends them on
/// the RFD form: an announcement, the launch prompt, the reply, the idle. A
/// second child fails before it answers.
fn root_updates() -> Vec<SessionNotification> {
    [
        serde_json::json!({
            "sessionUpdate": "subagent_update",
            "sessionId": "child",
            "title": "general-purpose",
            "description": "Fix the build",
            "state": { "state": "running" },
        }),
        serde_json::json!({
            "sessionUpdate": "session_message",
            "messageId": "toolu_1",
            "senderSessionId": "root",
            "recipientSessionId": "child",
            "content": [{ "type": "text", "text": "Make the build pass" }],
        }),
        serde_json::json!({
            "sessionUpdate": "session_message",
            "messageId": "toolu_1:reply",
            "senderSessionId": "child",
            "recipientSessionId": "root",
            "content": [{ "type": "text", "text": "Done." }],
        }),
        serde_json::json!({
            "sessionUpdate": "subagent_update",
            "sessionId": "child",
            "state": { "state": "idle", "stopReason": "end_turn" },
        }),
        serde_json::json!({
            "sessionUpdate": "subagent_update",
            "sessionId": "reviewer",
            "title": "Explore",
            "state": { "state": "running" },
        }),
        serde_json::json!({
            "sessionUpdate": "session_message",
            "messageId": "toolu_2",
            "senderSessionId": "root",
            "recipientSessionId": "reviewer",
            "content": [{ "type": "text", "text": "Review it" }],
        }),
        serde_json::json!({
            "sessionUpdate": "subagent_update",
            "sessionId": "reviewer",
            "state": { "state": "idle", "stopReason": "error" },
        }),
    ]
    .into_iter()
    .map(|update| {
        serde_json::from_value(serde_json::json!({ "sessionId": "root", "update": update }))
            .unwrap()
    })
    .collect()
}

/// Root session events the live projection emits.
#[derive(Default)]
struct RootEvents(Mutex<Vec<AgentEvent>>);

impl AgentSessionEventSink for RootEvents {
    fn session_update(&self, event: AgentEvent) -> Result<(), RuntimeError> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }

    fn config_options_changed(&self, _catalog: ConfigOptionsCatalog) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn commands_changed(&self, _catalog: AgentCommandsCatalog) -> Result<(), RuntimeError> {
        Ok(())
    }
}

/// Each Activity's identity, title and status, in first-seen order.
fn activities(messages: &[NormalizedMessage]) -> Vec<(String, String, ActivityStatus)> {
    let mut rows: Vec<(String, String, ActivityStatus)> = Vec::new();
    for message in messages {
        let NormalizedMessage::Activity { title, status, .. } = message else {
            continue;
        };
        let row = (message.identity().to_string(), title.clone(), *status);
        match rows.iter_mut().find(|existing| existing.0 == row.0) {
            Some(existing) => *existing = row,
            None => rows.push(row),
        }
    }
    rows
}

#[test]
fn replayed_delegation_rows_match_the_live_rows() {
    let replayed = ReplayProjection::for_agent("claude-code", "root").project(
        root_updates()
            .into_iter()
            .map(|notification| notification.update)
            .collect(),
    );

    let events = Arc::new(RootEvents::default());
    let sinks: crate::agent::acp_host_capabilities::AcpSessionEventSinkMap = Arc::default();
    sinks.lock().unwrap().insert("root".into(), events.clone());
    let router = AcpNativeSubagentRouter::new("claude-code", sinks);
    router.set_negotiated(true, Some("@agentclientprotocol/claude-agent-acp"));
    let root = LivePromptProjection::for_session("claude-code", events.clone());
    for notification in root_updates() {
        if let RoutedSessionNotification::Root(notification) = router.route(notification).unwrap() {
            root.emit(notification.update).unwrap();
        }
    }
    let live: Vec<_> = std::mem::take(&mut *events.0.lock().unwrap())
        .into_iter()
        .map(|event| normalize_event(event, "2026-01-01T00:00:00Z"))
        .collect();

    let expected = vec![
        (
            "acp_tool:toolu_1".to_string(),
            "Fix the build".to_string(),
            ActivityStatus::Completed,
        ),
        (
            "acp_tool:toolu_2".to_string(),
            "Explore".to_string(),
            ActivityStatus::Error,
        ),
    ];
    assert_eq!(activities(&replayed), expected);
    assert_eq!(activities(&live), expected);
    // Session messages add no other replayed rows.
    assert_eq!(replayed.len(), 2);
}
