use super::*;

#[derive(Default)]
struct CaptureSink {
    spawns: Mutex<Vec<AgentNativeSubagentSpawned>>,
    updates: Mutex<Vec<AgentEvent>>,
    details: Mutex<Vec<AgentNativeSubagentDetailsUpdate>>,
}

impl AgentSessionEventSink for CaptureSink {
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

    fn subagent_spawned(&self, event: AgentNativeSubagentSpawned) -> Result<(), RuntimeError> {
        self.spawns.lock().unwrap().push(event);
        Ok(())
    }

    fn subagent_details_changed(
        &self,
        event: AgentNativeSubagentDetailsUpdate,
    ) -> Result<(), RuntimeError> {
        self.details.lock().unwrap().push(event);
        Ok(())
    }
}

fn claude_router(capture: &Arc<CaptureSink>) -> AcpNativeSubagentRouter {
    let sinks: AcpSessionEventSinkMap = Arc::default();
    sinks.lock().unwrap().insert("root".into(), capture.clone());
    let router = AcpNativeSubagentRouter::new("claude-code", sinks);
    router.set_negotiated(true, Some("@agentclientprotocol/claude-agent-acp"));
    router
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
    let spawn = || {
        router.remember_spawn_prompt(&serde_json::json!({
            "sessionId": "root",
            "update": {
                "sessionUpdate": "subagent_spawned",
                "subagentSessionId": "child",
                "name": "Researcher",
                "task": "Summary",
                "prompt": "Read the project guide",
            },
        }));
        router
            .route(SessionNotification::new(
                "root",
                SessionUpdate::SubagentSpawned(SubagentSpawnedUpdate::new(
                    "child",
                    "Researcher",
                    "Summary",
                    Default::default(),
                )),
            ))
            .unwrap();
    };
    spawn();
    spawn();

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
    router.remember_spawn_prompt(&serde_json::json!({
        "sessionId": "root",
        "update": { "sessionUpdate": "subagent_spawned", "subagentSessionId": "child" },
    }));
    router
        .route(SessionNotification::new(
            "root",
            SessionUpdate::SubagentSpawned(SubagentSpawnedUpdate::new(
                "child",
                "Researcher",
                "Summary",
                Default::default(),
            )),
        ))
        .unwrap();
    assert!(capture.updates.lock().unwrap().is_empty());
}

#[test]
fn claude_spawn_metadata_becomes_typed_details() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    router
        .route(SessionNotification::new(
            "root",
            SessionUpdate::SubagentSpawned(
                SubagentSpawnedUpdate::new("child", "Researcher", "Summary", Default::default())
                    .meta(claude_meta(serde_json::json!({
                        "type": "Explore",
                        "requestedModel": "sonnet",
                        "unknown": "ignored",
                    }))),
            ),
        ))
        .unwrap();
    assert_eq!(
        detail_pairs(&capture.spawns.lock().unwrap()[0].details),
        [("Agent type", "Explore"), ("Model", "sonnet")],
    );
}

#[test]
fn child_session_info_reports_the_resolved_model() {
    let capture = Arc::new(CaptureSink::default());
    let router = claude_router(&capture);
    router
        .route(SessionNotification::new(
            "root",
            SessionUpdate::SubagentSpawned(SubagentSpawnedUpdate::new(
                "child",
                "Researcher",
                "Summary",
                Default::default(),
            )),
        ))
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
        let notification = SessionNotification::new(
            "root",
            SessionUpdate::SubagentSpawned(SubagentSpawnedUpdate::new(
                "child",
                "Researcher",
                "Delegated task for Researcher",
                Default::default(),
            )),
        );
        assert!(matches!(
            router.route(notification).unwrap(),
            RoutedSessionNotification::Handled,
        ));
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
    let sinks: AcpSessionEventSinkMap = Arc::default();
    sinks.lock().unwrap().insert("root".into(), capture.clone());
    let router = AcpNativeSubagentRouter::new("custom.codex-subagent-fix", sinks);
    router.set_negotiated(true, Some("@openaide/codex-acp"));
    for _ in 0..2 {
        router
            .route(SessionNotification::new(
                "root",
                SessionUpdate::SubagentSpawned(SubagentSpawnedUpdate::new(
                    "child",
                    "Researcher",
                    "Delegated task for Researcher",
                    Default::default(),
                )),
            ))
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
fn unrelated_custom_adapter_retains_real_child_prompts() {
    let capture = Arc::new(CaptureSink::default());
    let sinks: AcpSessionEventSinkMap = Arc::default();
    sinks.lock().unwrap().insert("root".into(), capture.clone());
    // A Codex-like configured id must not select another implementation's policy.
    let router = AcpNativeSubagentRouter::new("custom.codex-example", sinks);
    router.set_negotiated(true, Some("other-agent"));
    router
        .route(SessionNotification::new(
            "root",
            SessionUpdate::SubagentSpawned(SubagentSpawnedUpdate::new(
                "child",
                "Researcher",
                "Read the project guide",
                Default::default(),
            )),
        ))
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
