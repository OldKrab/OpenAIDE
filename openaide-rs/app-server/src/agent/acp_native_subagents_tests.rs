use super::*;

#[derive(Default)]
struct CaptureSink {
    spawns: Mutex<Vec<AgentNativeSubagentSpawned>>,
    updates: Mutex<Vec<AgentEvent>>,
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
