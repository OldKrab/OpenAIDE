use std::sync::Arc;

use super::super::tests::{claude_router, CaptureSink};
use super::super::{AcpNativeSubagentRouter, RoutedSessionNotification};
use crate::agent::acp_schema::SessionNotification;
use crate::agent::events::{AgentEvent, AgentNativeSubagentState};

fn route(
    router: &AcpNativeSubagentRouter,
    session_id: &str,
    update: serde_json::Value,
) -> Result<RoutedSessionNotification, crate::protocol::errors::RuntimeError> {
    let notification: SessionNotification =
        serde_json::from_value(serde_json::json!({ "sessionId": session_id, "update": update }))
            .unwrap();
    router.route(notification)
}

fn subagent_update(fields: serde_json::Value) -> serde_json::Value {
    let mut update = serde_json::json!({
        "sessionUpdate": "subagent_update",
        "sessionId": "child",
    });
    for (key, value) in fields.as_object().unwrap() {
        update[key] = value.clone();
    }
    update
}

fn announce(router: &AcpNativeSubagentRouter) {
    route(
        router,
        "root",
        subagent_update(serde_json::json!({
            "title": "general-purpose",
            "description": "Fix the build",
            "capabilities": { "cancel": {} },
            "state": { "state": "running" },
        })),
    )
    .unwrap();
}

fn idle(stop_reason: Option<&str>) -> serde_json::Value {
    let mut state = serde_json::json!({ "state": "idle" });
    if let Some(stop_reason) = stop_reason {
        state["stopReason"] = stop_reason.into();
    }
    subagent_update(serde_json::json!({ "state": state }))
}

/// Replays the traffic shape `@openaide/claude-agent-acp` 1.7.0 sends for one
/// delegated run, with synthetic text.
#[test]
fn claude_released_run_becomes_one_subagent_with_its_prompt_and_reply() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    announce(&router);
    route(
        &router,
        "child",
        serde_json::json!({
            "sessionUpdate": "session_message",
            "messageId": "toolu_1",
            "senderSessionId": "root",
            "recipientSessionId": "child",
            "content": [{ "type": "text", "text": "Make the build pass" }],
        }),
    )
    .unwrap();
    route(
        &router,
        "child",
        serde_json::json!({
            "sessionUpdate": "session_info_update",
            "_meta": { "claudeCode": { "nativeSubagent": {
                "type": "general-purpose", "model": "claude-sonnet-5-5"
            } } },
        }),
    )
    .unwrap();
    route(
        &router,
        "child",
        serde_json::json!({
            "sessionUpdate": "agent_message_chunk",
            "messageId": "msg_1",
            "content": { "type": "text", "text": "Done." },
        }),
    )
    .unwrap();
    route(&router, "root", idle(Some("end_turn"))).unwrap();

    let spawns = capture.spawns.lock().unwrap();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].parent_native_session_id, "root");
    assert_eq!(spawns[0].native_session_id, "child");
    assert_eq!(spawns[0].name, "general-purpose");
    assert_eq!(spawns[0].delegated_task.as_deref(), Some("Fix the build"));
    assert!(spawns[0].parent_records_delegation);
    assert!(!spawns[0].parent_interaction);
    assert!(spawns[0].capabilities.cancel);
    let updates = capture.updates.lock().unwrap();
    assert!(matches!(
        updates.as_slice(),
        [
            AgentEvent::UserMessageChunk { text, source_message_id },
            AgentEvent::MessageChunk { .. },
        ] if text == "Make the build pass" && source_message_id.as_deref() == Some("toolu_1")
    ));
    assert_eq!(capture.details.lock().unwrap().len(), 1);
    assert_eq!(
        *capture.states.lock().unwrap(),
        [
            AgentNativeSubagentState::Running,
            AgentNativeSubagentState::Completed
        ]
    );
}

#[test]
fn a_reused_child_runs_again_and_repeated_work_states_apply_once() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    announce(&router);
    route(&router, "root", idle(Some("end_turn"))).unwrap();
    for state in ["running", "requires_action", "running"] {
        route(
            &router,
            "root",
            subagent_update(serde_json::json!({ "state": { "state": state } })),
        )
        .unwrap();
    }
    route(&router, "root", idle(Some("cancelled"))).unwrap();

    assert_eq!(capture.spawns.lock().unwrap().len(), 1);
    assert_eq!(
        *capture.states.lock().unwrap(),
        [
            AgentNativeSubagentState::Running,
            AgentNativeSubagentState::Completed,
            AgentNativeSubagentState::Running,
            AgentNativeSubagentState::Cancelled,
        ]
    );
}

#[test]
fn idle_stop_reasons_and_unknown_state_map_to_product_statuses() {
    let cases = [
        (idle(None), AgentNativeSubagentState::Completed),
        (idle(Some("end_turn")), AgentNativeSubagentState::Completed),
        (idle(Some("cancelled")), AgentNativeSubagentState::Cancelled),
        (idle(Some("error")), AgentNativeSubagentState::Failed),
        (idle(Some("refusal")), AgentNativeSubagentState::Failed),
        (idle(Some("max_tokens")), AgentNativeSubagentState::Failed),
        (
            idle(Some("max_turn_requests")),
            AgentNativeSubagentState::Failed,
        ),
        (
            idle(Some("a_future_reason")),
            AgentNativeSubagentState::Failed,
        ),
        (
            subagent_update(serde_json::json!({ "state": { "state": "unknown" } })),
            AgentNativeSubagentState::Disconnected,
        ),
    ];
    for (update, expected) in cases {
        let capture = Arc::new(CaptureSink::default());
        let router = claude_router(&capture);
        announce(&router);
        route(&router, "root", update.clone()).unwrap();
        assert_eq!(
            capture.states.lock().unwrap().last(),
            Some(&expected),
            "{update}"
        );
    }
}

#[test]
fn later_updates_patch_identity_and_null_clears() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    announce(&router);
    route(
        &router,
        "root",
        subagent_update(serde_json::json!({
            "title": "reviewer (Explore)",
        })),
    )
    .unwrap();
    route(
        &router,
        "root",
        subagent_update(serde_json::json!({
            "title": null,
            "description": null,
            "capabilities": null,
        })),
    )
    .unwrap();
    // Only a state change: no identity patch.
    route(&router, "root", idle(None)).unwrap();

    let identities = capture.identities.lock().unwrap();
    assert_eq!(identities.len(), 2);
    assert_eq!(identities[0].name.as_deref(), Some("reviewer (Explore)"));
    assert_eq!(identities[0].delegated_task, None);
    assert_eq!(identities[0].cancel, None);
    assert_eq!(identities[1].name.as_deref(), Some("Subagent"));
    assert_eq!(identities[1].delegated_task.as_deref(), Some(""));
    assert_eq!(identities[1].cancel, Some(false));
    assert_eq!(router.child_attribution("child").unwrap().1, "Subagent");
}

#[test]
fn untitled_child_without_cancel_gets_defaults() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    route(&router, "root", subagent_update(serde_json::json!({}))).unwrap();

    let spawns = capture.spawns.lock().unwrap();
    assert_eq!(spawns[0].name, "Subagent");
    assert_eq!(spawns[0].delegated_task, None);
    assert!(!spawns[0].capabilities.cancel);
    assert!(capture.states.lock().unwrap().is_empty());
}

#[test]
fn an_update_from_another_parent_or_without_negotiation_is_rejected() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    announce(&router);
    capture.spawns.lock().unwrap().clear();
    router
        .sinks
        .lock()
        .unwrap()
        .insert("other".into(), capture.clone());
    assert!(route(&router, "other", idle(None)).is_err());

    router.set_negotiated(false, None);
    assert!(route(
        &router,
        "root",
        subagent_update(serde_json::json!({ "state": { "state": "running" } })),
    )
    .is_err());
    assert!(capture.spawns.lock().unwrap().is_empty());
}

#[test]
fn only_messages_sent_to_the_child_become_its_user_messages() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    announce(&router);
    for (sender, recipient) in [("child", "root"), ("root", "sibling")] {
        let routed = route(
            &router,
            "child",
            serde_json::json!({
                "sessionUpdate": "session_message",
                "messageId": "m1",
                "senderSessionId": sender,
                "recipientSessionId": recipient,
                "content": [{ "type": "text", "text": "hello" }],
            }),
        )
        .unwrap();
        assert!(matches!(routed, RoutedSessionNotification::Handled));
    }
    route(
        &router,
        "child",
        serde_json::json!({
            "sessionUpdate": "session_message_chunk",
            "messageId": "m2",
            "recipientSessionId": "child",
            "content": { "type": "text", "text": "chunk" },
        }),
    )
    .unwrap();
    // A root session's own message is never projected.
    let routed = route(
        &router,
        "root",
        serde_json::json!({
            "sessionUpdate": "session_message",
            "messageId": "m3",
            "content": [{ "type": "text", "text": "to someone" }],
        }),
    )
    .unwrap();
    assert!(matches!(routed, RoutedSessionNotification::Handled));

    let updates = capture.updates.lock().unwrap();
    assert!(matches!(
        updates.as_slice(),
        [AgentEvent::UserMessageChunk { text, source_message_id }]
            if text == "chunk" && source_message_id.as_deref() == Some("m2")
    ));
}
