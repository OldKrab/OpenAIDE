use std::sync::Arc;

use super::super::tests::{claude_router, CaptureSink};
use super::super::{AcpNativeSubagentRouter, RoutedSessionNotification};
use crate::agent::acp_live_prompt_projection::LivePromptProjection;
use crate::agent::acp_schema::SessionNotification;
use crate::agent::events::{
    AgentEvent, AgentNativeSubagentState, AgentToolCall, AgentToolCallStatus,
};

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
            message("m1", Some(sender), Some(recipient), "hello"),
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
    // A root message that names no child is still not projected.
    let routed = route(&router, "root", message("m3", None, None, "to someone")).unwrap();
    assert!(matches!(routed, RoutedSessionNotification::Handled));
    // The root's own message to the child is its delegation row.
    let routed = route(
        &router,
        "root",
        message("m4", Some("root"), Some("child"), "Make the build pass"),
    )
    .unwrap();
    assert!(matches!(routed, RoutedSessionNotification::Root(_)));

    let updates = capture.updates.lock().unwrap();
    assert!(matches!(
        updates.as_slice(),
        [AgentEvent::UserMessageChunk { text, source_message_id }]
            if text == "chunk" && source_message_id.as_deref() == Some("m2")
    ));
}

fn message(
    message_id: &str,
    sender: Option<&str>,
    recipient: Option<&str>,
    text: &str,
) -> serde_json::Value {
    let mut message = serde_json::json!({
        "sessionUpdate": "session_message",
        "messageId": message_id,
        "content": [{ "type": "text", "text": text }],
    });
    if let Some(sender) = sender {
        message["senderSessionId"] = sender.into();
    }
    if let Some(recipient) = recipient {
        message["recipientSessionId"] = recipient.into();
    }
    message
}

/// The root session's projection: where a `Root` notification goes next.
fn root_projection(capture: &Arc<CaptureSink>) -> LivePromptProjection {
    LivePromptProjection::for_session("claude-code", capture.clone())
}

/// Routes `update` on `session_id` and projects a root result the way the
/// root session does. Returns whether the router returned a root update.
fn route_through(
    router: &AcpNativeSubagentRouter,
    root: &LivePromptProjection,
    session_id: &str,
    update: serde_json::Value,
) -> bool {
    match route(router, session_id, update).unwrap() {
        RoutedSessionNotification::Root(notification) => {
            assert_eq!(&*notification.session_id.0, session_id);
            root.emit(notification.update).unwrap();
            true
        }
        RoutedSessionNotification::Handled => false,
    }
}

/// The current state of each Tool row in `events`, in first-seen order.
fn tool_rows(events: &[AgentEvent]) -> Vec<AgentToolCall> {
    let mut rows: Vec<AgentToolCall> = Vec::new();
    for event in events {
        let AgentEvent::ToolCall(tool) = event else {
            continue;
        };
        match rows
            .iter_mut()
            .find(|row| row.tool_call_id == tool.tool_call_id)
        {
            Some(row) => *row = tool.clone(),
            None => rows.push(tool.clone()),
        }
    }
    rows
}

fn root_rows(capture: &CaptureSink) -> Vec<AgentToolCall> {
    tool_rows(&capture.root_updates.lock().unwrap())
}

/// The adapter's two views of a launch: the parent's outgoing message, then
/// the child's incoming one.
fn launch(router: &AcpNativeSubagentRouter, root: &LivePromptProjection, id: &str) {
    let prompt = message(id, Some("root"), Some("child"), "Make the build pass");
    assert!(route_through(router, root, "root", prompt.clone()));
    assert!(!route_through(router, root, "child", prompt));
}

#[test]
fn a_launch_and_its_reply_are_one_parent_row_that_completes() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    let root = root_projection(&capture);
    announce(&router);
    launch(&router, &root, "toolu_1");
    let rows = root_rows(&capture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tool_call_id, "toolu_1");
    assert_eq!(rows[0].title, "Fix the build");
    assert_eq!(rows[0].kind, "think");
    assert_eq!(rows[0].status, AgentToolCallStatus::InProgress);
    assert_eq!(
        rows[0].output_preview.as_deref(),
        Some("Make the build pass")
    );

    assert!(route_through(
        &router,
        &root,
        "root",
        message("toolu_1:reply", Some("child"), Some("root"), "Done."),
    ));
    // The reply already completed the row; the idle closes nothing.
    assert!(!route_through(
        &router,
        &root,
        "root",
        idle(Some("end_turn"))
    ));

    let rows = root_rows(&capture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, AgentToolCallStatus::Completed);
    assert_eq!(rows[0].title, "Fix the build");
    assert_eq!(rows[0].output_preview.as_deref(), Some("Done."));
    // The child still opens its history with the prompt.
    assert!(matches!(
        capture.updates.lock().unwrap().as_slice(),
        [AgentEvent::UserMessageChunk { text, .. }] if text == "Make the build pass"
    ));
}

#[test]
fn a_reply_after_the_idle_still_becomes_the_row_content() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    let root = root_projection(&capture);
    announce(&router);
    launch(&router, &root, "toolu_1");
    assert!(route_through(
        &router,
        &root,
        "root",
        idle(Some("end_turn"))
    ));
    assert!(route_through(
        &router,
        &root,
        "root",
        message("toolu_1:reply", Some("child"), Some("root"), "Done."),
    ));

    let rows = root_rows(&capture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, AgentToolCallStatus::Completed);
    assert_eq!(rows[0].output_preview.as_deref(), Some("Done."));
}

#[test]
fn a_message_to_a_launched_child_is_a_row_completed_on_delivery() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    let root = root_projection(&capture);
    announce(&router);
    launch(&router, &root, "toolu_1");
    route_through(
        &router,
        &root,
        "root",
        message("toolu_1:reply", Some("child"), Some("root"), "Done."),
    );
    route_through(&router, &root, "root", idle(Some("end_turn")));
    let send = message("toolu_2", Some("root"), Some("child"), "Try again");
    assert!(route_through(&router, &root, "root", send.clone()));
    assert!(!route_through(&router, &root, "child", send));

    let rows = root_rows(&capture);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].tool_call_id, "toolu_2");
    assert_eq!(rows[1].title, "Message to general-purpose");
    assert_eq!(rows[1].status, AgentToolCallStatus::Completed);
    assert_eq!(rows[1].output_preview.as_deref(), Some("Try again"));
    // The launch row is untouched by the later message.
    assert_eq!(rows[0].output_preview.as_deref(), Some("Done."));
}

#[test]
fn a_handback_completes_the_open_launch_and_a_later_one_is_its_own_row() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    let root = root_projection(&capture);
    announce(&router);
    launch(&router, &root, "toolu_1");
    // The adapter's two views of a hand-back: the child's outgoing one, then
    // the parent's incoming one.
    let handback = message("toolu_h1", Some("child"), Some("root"), "Report");
    assert!(!route_through(&router, &root, "child", handback.clone()));
    assert!(route_through(&router, &root, "root", handback));
    assert!(!route_through(
        &router,
        &root,
        "root",
        idle(Some("end_turn"))
    ));
    assert!(route_through(
        &router,
        &root,
        "root",
        message("toolu_h2", Some("child"), Some("root"), "Second report"),
    ));

    let rows = root_rows(&capture);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].tool_call_id, "toolu_1");
    assert_eq!(rows[0].status, AgentToolCallStatus::Completed);
    assert_eq!(rows[0].output_preview.as_deref(), Some("Report"));
    assert_eq!(rows[1].tool_call_id, "toolu_h2");
    assert_eq!(rows[1].title, "Message from general-purpose");
    assert_eq!(rows[1].status, AgentToolCallStatus::Completed);
}

#[test]
fn a_terminal_idle_closes_the_open_launch_row() {
    let cases = [
        (idle(Some("end_turn")), AgentToolCallStatus::Completed),
        (idle(Some("error")), AgentToolCallStatus::Failed),
        (idle(Some("cancelled")), AgentToolCallStatus::Failed),
        (
            subagent_update(serde_json::json!({ "state": { "state": "unknown" } })),
            AgentToolCallStatus::Failed,
        ),
    ];
    for (update, expected) in cases {
        let capture = Arc::new(CaptureSink::default());
        let router = claude_router(&capture);
        let root = root_projection(&capture);
        announce(&router);
        launch(&router, &root, "toolu_1");
        assert!(route_through(&router, &root, "root", update.clone()));
        // Work that runs again does not reopen the closed launch.
        let running = subagent_update(serde_json::json!({ "state": { "state": "running" } }));
        assert!(!route_through(&router, &root, "root", running));
        assert!(!route_through(&router, &root, "root", update.clone()));

        let rows = root_rows(&capture);
        assert_eq!(rows.len(), 1, "{update}");
        assert_eq!(rows[0].status, expected, "{update}");
    }
}

#[test]
fn a_grandchild_delegation_is_a_row_in_the_child_history_not_a_user_message() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    let root = root_projection(&capture);
    announce(&router);
    route(
        &router,
        "child",
        serde_json::json!({
            "sessionUpdate": "subagent_update",
            "sessionId": "grandchild",
            "title": "Explore",
            "state": { "state": "running" },
        }),
    )
    .unwrap();
    for update in [
        message("toolu_g", Some("child"), Some("grandchild"), "Look around"),
        message(
            "toolu_g:reply",
            Some("grandchild"),
            Some("child"),
            "Found it",
        ),
        message("toolu_gh", Some("grandchild"), Some("child"), "Late report"),
    ] {
        assert!(!route_through(&router, &root, "child", update));
    }

    // Nothing reaches the root transcript, and the child history has rows only.
    assert!(root_rows(&capture).is_empty());
    let updates = capture.updates.lock().unwrap();
    assert!(!updates
        .iter()
        .any(|event| matches!(event, AgentEvent::UserMessageChunk { .. })));
    let rows = tool_rows(&updates);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].tool_call_id, "toolu_g");
    assert_eq!(rows[0].title, "Explore");
    assert_eq!(rows[0].status, AgentToolCallStatus::Completed);
    assert_eq!(rows[0].output_preview.as_deref(), Some("Found it"));
    assert_eq!(rows[1].tool_call_id, "toolu_gh");
    assert_eq!(rows[1].title, "Message from Explore");
}

#[test]
fn messages_without_a_known_launch_or_child_are_ignored() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    let root = root_projection(&capture);
    announce(&router);
    for update in [
        // Unknown recipient and unknown sender.
        message("m1", Some("root"), Some("stranger"), "hello"),
        message("m2", Some("stranger"), Some("root"), "hello"),
        // A reply to a launch this router never saw.
        message("toolu_0:reply", Some("child"), Some("root"), "Done."),
        // A part of a message cannot open a row.
        serde_json::json!({
            "sessionUpdate": "session_message_chunk",
            "messageId": "m3",
            "senderSessionId": "root",
            "recipientSessionId": "child",
            "content": { "type": "text", "text": "part" },
        }),
    ] {
        assert!(
            !route_through(&router, &root, "root", update.clone()),
            "{update}"
        );
    }
    launch(&router, &root, "toolu_1");
    // The same launch again is not a second row or a message.
    let repeated = message(
        "toolu_1",
        Some("root"),
        Some("child"),
        "Make the build pass",
    );
    assert!(!route_through(&router, &root, "root", repeated));

    let rows = root_rows(&capture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tool_call_id, "toolu_1");
}

#[test]
fn the_agent_tool_call_shown_for_a_permission_keeps_its_row() {
    use crate::agent::acp_schema::{SessionUpdate, ToolCall, ToolCallStatus, ToolKind};

    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    let root = root_projection(&capture);
    // The adapter showed the launch's real tool call before the child was known.
    root.emit(SessionUpdate::ToolCall(
        ToolCall::new("toolu_1", "Agent call")
            .kind(ToolKind::Think)
            .status(ToolCallStatus::InProgress),
    ))
    .unwrap();
    announce(&router);
    assert!(route_through(
        &router,
        &root,
        "root",
        message(
            "toolu_1",
            Some("root"),
            Some("child"),
            "Make the build pass"
        ),
    ));

    let rows = root_rows(&capture);
    assert_eq!(capture.root_updates.lock().unwrap().len(), 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Agent call");
}
