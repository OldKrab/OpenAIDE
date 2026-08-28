use super::*;

#[derive(Default)]
struct CaptureSink {
    spawns: Mutex<Vec<AgentNativeSubagentSpawned>>,
}

impl AgentSessionEventSink for CaptureSink {
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
    router.set_negotiated(true);

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
