use super::*;

#[derive(Default)]
pub(super) struct CaptureSink {
    pub(super) spawns: Mutex<Vec<AgentNativeSubagentSpawned>>,
    pub(super) updates: Mutex<Vec<AgentEvent>>,
    pub(super) details: Mutex<Vec<AgentNativeSubagentDetailsUpdate>>,
    pub(super) states: Mutex<Vec<AgentNativeSubagentState>>,
    pub(super) identities: Mutex<Vec<crate::agent::events::AgentNativeSubagentIdentityUpdate>>,
    /// What a root session projection emitted.
    pub(super) root_updates: Mutex<Vec<AgentEvent>>,
    /// The child the Task stores from an earlier connection.
    pub(super) stored: Mutex<Option<crate::agent::events::AgentNativeSubagentStoredIdentity>>,
}

impl AgentSessionEventSink for CaptureSink {
    fn session_update(&self, event: AgentEvent) -> Result<(), RuntimeError> {
        self.root_updates.lock().unwrap().push(event);
        Ok(())
    }

    fn subagent_session_update(
        &self,
        _session_id: &str,
        event: AgentEvent,
    ) -> Result<(), RuntimeError> {
        self.updates.lock().unwrap().push(event);
        Ok(())
    }

    fn config_options_changed(
        &self,
        _catalog: crate::protocol::model::ConfigOptionsCatalog,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn commands_changed(
        &self,
        _catalog: crate::protocol::model::AgentCommandsCatalog,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn stored_subagent_identity(
        &self,
        _native_session_id: &str,
    ) -> Result<Option<crate::agent::events::AgentNativeSubagentStoredIdentity>, RuntimeError> {
        Ok(self.stored.lock().unwrap().clone())
    }

    fn subagent_spawned(&self, event: AgentNativeSubagentSpawned) -> Result<(), RuntimeError> {
        self.spawns.lock().unwrap().push(event);
        Ok(())
    }

    fn subagent_state_changed(
        &self,
        event: AgentNativeSubagentStateUpdate,
    ) -> Result<(), RuntimeError> {
        self.states.lock().unwrap().push(event.state);
        Ok(())
    }

    fn subagent_details_changed(
        &self,
        event: AgentNativeSubagentDetailsUpdate,
    ) -> Result<(), RuntimeError> {
        self.details.lock().unwrap().push(event);
        Ok(())
    }

    fn subagent_identity_changed(
        &self,
        event: crate::agent::events::AgentNativeSubagentIdentityUpdate,
    ) -> Result<(), RuntimeError> {
        self.identities.lock().unwrap().push(event);
        Ok(())
    }
}

pub(super) fn claude_router(capture: &Arc<CaptureSink>) -> AcpNativeSubagentRouter {
    router_for(
        capture,
        "claude-code",
        "@agentclientprotocol/claude-agent-acp",
    )
}

fn codex_router(capture: &Arc<CaptureSink>) -> AcpNativeSubagentRouter {
    router_for(capture, "custom.codex-subagent-fix", "@openaide/codex-acp")
}

pub(super) fn router_for(
    capture: &Arc<CaptureSink>,
    agent_id: &str,
    implementation_name: &str,
) -> AcpNativeSubagentRouter {
    let sinks: AcpSessionEventSinkMap = Arc::default();
    sinks.lock().unwrap().insert("root".into(), capture.clone());
    let router = AcpNativeSubagentRouter::new(agent_id, sinks);
    router.set_negotiated(true, Some(implementation_name));
    router
}

fn draft(session_id: &str, update: serde_json::Value) -> DraftSubagentNotification {
    let message = agent_client_protocol::UntypedMessage::new(
        "session/update",
        serde_json::json!({ "sessionId": session_id, "update": update }),
    )
    .unwrap();
    crate::agent::acp_draft_subagents::decode_draft_subagent_update(&message)
        .unwrap()
        .unwrap()
}

fn draft_spawn(meta: serde_json::Value) -> DraftSubagentNotification {
    draft_spawn_with(serde_json::json!({ "_meta": meta }))
}

/// A draft announcement of `child` by `root`, with `fields` added or replaced.
fn draft_spawn_with(fields: serde_json::Value) -> DraftSubagentNotification {
    let mut update = serde_json::json!({
        "sessionUpdate": "subagent_spawned",
        "subagentSessionId": "child",
        "name": "Researcher",
        "task": "Delegated task for Researcher",
        "capabilities": {},
    });
    for (key, value) in fields.as_object().unwrap() {
        update[key] = value.clone();
    }
    draft("root", update)
}

fn claude_meta(subagent: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({ "claudeCode": { "nativeSubagent": subagent } })
        .as_object()
        .unwrap()
        .clone()
}

fn detail_pairs(details: &[AgentNativeSubagentDetail]) -> Vec<(&str, &str)> {
    details
        .iter()
        .map(|detail| (detail.label.as_str(), detail.value.as_str()))
        .collect()
}

#[test]
fn exact_spawn_prompt_opens_the_child_history_once() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    for _ in 0..2 {
        router
            .route_draft(draft_spawn_with(serde_json::json!({
                "task": "Summary",
                "prompt": "Read the project guide",
            })))
            .unwrap();
    }

    let updates = capture.updates.lock().unwrap();
    assert_eq!(
        updates.len(),
        1,
        "a repeated announcement adds no prompt row"
    );
    assert!(matches!(
        &updates[0],
        AgentEvent::UserMessageChunk { text, .. } if text == "Read the project guide"
    ));
}

#[test]
fn spawn_without_the_prompt_extension_adds_no_user_message() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    router
        .route_draft(draft_spawn_with(serde_json::json!({ "prompt": "  " })))
        .unwrap();
    assert!(capture.updates.lock().unwrap().is_empty());
}

#[test]
fn claude_spawn_metadata_becomes_typed_details() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    router
        .route_draft(draft_spawn(serde_json::Value::Object(claude_meta(
            serde_json::json!({
                "type": "Explore",
                "requestedModel": "sonnet",
                "unknown": "ignored",
            }),
        ))))
        .unwrap();
    assert_eq!(
        detail_pairs(&capture.spawns.lock().unwrap()[0].details),
        [("Agent type", "Explore"), ("Model", "sonnet")],
    );
}

#[test]
fn codex_spawn_metadata_becomes_typed_details() {
    let capture = Arc::new(CaptureSink::default());
    let router = codex_router(&capture);
    router
        .route_draft(draft_spawn(serde_json::json!({
            "codex": {
                "subagent": { "path": "/root/researcher" },
                "collaboration": { "model": "gpt-5", "unknown": "ignored" },
            },
        })))
        .unwrap();
    assert_eq!(
        detail_pairs(&capture.spawns.lock().unwrap()[0].details),
        [("Path", "/root/researcher"), ("Model", "gpt-5")],
    );
}

#[test]
fn child_session_info_reports_the_resolved_model() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    router
        .route_draft(draft_spawn(serde_json::Value::Null))
        .unwrap();
    let info = |session_id: &str| {
        SessionNotification::new(
            session_id.to_string(),
            SessionUpdate::SessionInfoUpdate(
                crate::agent::acp_schema::SessionInfoUpdate::new().meta(claude_meta(
                    serde_json::json!({ "model": "claude-sonnet-5-5", "requestedModel": "sonnet" }),
                )),
            ),
        )
    };
    assert!(matches!(
        router.route(info("child")).unwrap(),
        RoutedSessionNotification::Handled,
    ));
    // The same update on the root session keeps its ordinary Task metadata meaning.
    assert!(matches!(
        router.route(info("root")).unwrap(),
        RoutedSessionNotification::Root(_),
    ));

    let details = capture.details.lock().unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].native_session_id, "child");
    assert_eq!(
        detail_pairs(&details[0].details),
        [("Model", "claude-sonnet-5-5")],
    );
}

#[test]
fn repeated_codex_child_announcement_marks_a_parent_interaction() {
    let capture = Arc::new(CaptureSink::default());
    let sinks: AcpSessionEventSinkMap = Arc::default();
    sinks
        .lock()
        .unwrap()
        .insert("root".to_string(), capture.clone());
    let router = AcpNativeSubagentRouter::new("codex", sinks);
    router.set_negotiated(true, None);

    for _ in 0..2 {
        router
            .route_draft(draft_spawn(serde_json::Value::Null))
            .unwrap();
    }

    let spawns = capture.spawns.lock().unwrap();
    assert_eq!(spawns.len(), 2);
    assert!(!spawns[0].parent_interaction);
    assert!(spawns[1].parent_interaction);
    assert!(spawns.iter().all(|spawn| spawn.delegated_task.is_none()));
}

#[test]
fn custom_codex_identity_preserves_interactions_without_placeholder_prompts() {
    let capture = Arc::new(CaptureSink::default());
    let router = codex_router(&capture);
    for _ in 0..2 {
        router
            .route_draft(draft_spawn(serde_json::Value::Null))
            .unwrap();
    }
    router
        .route(SessionNotification::new(
            "child",
            SessionUpdate::UserMessageChunk(crate::agent::acp_schema::ContentChunk::new(
                crate::agent::acp_schema::ContentBlock::Text(
                    crate::agent::acp_schema::TextContent::new("Causal root placeholder"),
                ),
            )),
        ))
        .unwrap();
    let spawns = capture.spawns.lock().unwrap();
    assert!(spawns.iter().all(|event| event.delegated_task.is_none()));
    assert!(!spawns[0].parent_interaction);
    assert!(spawns[1].parent_interaction);
    assert!(capture.updates.lock().unwrap().is_empty());
}

#[test]
fn codex_terminal_state_reaches_the_root_session() {
    let capture = Arc::new(CaptureSink::default());
    let router = codex_router(&capture);
    router
        .route_draft(draft_spawn(serde_json::Value::Null))
        .unwrap();
    router
        .route_draft(draft(
            "root",
            serde_json::json!({
                "sessionUpdate": "subagent_state_update",
                "subagentSessionId": "child",
                "state": "cancelled",
            }),
        ))
        .unwrap();
    assert_eq!(
        capture.states.lock().unwrap().as_slice(),
        [AgentNativeSubagentState::Cancelled],
    );
}

#[test]
fn unrelated_custom_adapter_retains_real_child_prompts() {
    let capture = Arc::new(CaptureSink::default());
    // A Codex-like configured id must not select another implementation's policy.
    let router = router_for(&capture, "custom.codex-example", "other-agent");
    router
        .route_draft(draft_spawn_with(serde_json::json!({
            "task": "Read the project guide",
        })))
        .unwrap();
    router
        .route(SessionNotification::new(
            "child",
            SessionUpdate::UserMessageChunk(crate::agent::acp_schema::ContentChunk::new(
                crate::agent::acp_schema::ContentBlock::Text(
                    crate::agent::acp_schema::TextContent::new("Read the project guide"),
                ),
            )),
        ))
        .unwrap();
    assert_eq!(
        capture.spawns.lock().unwrap()[0].delegated_task.as_deref(),
        Some("Read the project guide")
    );
    assert!(
        matches!(&capture.updates.lock().unwrap()[0], AgentEvent::UserMessageChunk { text, .. } if text == "Read the project guide")
    );
}
