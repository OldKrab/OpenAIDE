use super::*;

struct SlowPreferencesAgent {
    catalog_on_attach: bool,
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl AgentRuntime for SlowPreferencesAgent {
    fn prompt(
        &self,
        _: AgentPrompt,
        _: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
    }

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        let session = AgentSession::new(request.agent_id, "preferences-session");
        Ok(if self.catalog_on_attach {
            session
        } else {
            session.with_config_options(&mode_config_catalog("agent"))
        })
    }

    fn attach_session_event_sink(
        &self,
        _: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        if self.catalog_on_attach {
            sink.config_options_changed(mode_config_catalog("agent"))?;
        }
        Ok(())
    }

    fn set_session_config_option(
        &self,
        _request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.entered.send(()).unwrap();
        let _ = self
            .release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5));
        Ok(mode_config_catalog("agent-full-access"))
    }
}

#[test]
fn live_options_are_visible_before_saved_preferences_finish() {
    for catalog_on_attach in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let workspace = "/tmp/openaide-unit-workspace/app";
        store.write_task(&task_record("anchor", workspace)).unwrap();
        store
            .write_agent_config_preferences(&mode_config_catalog("agent-full-access"))
            .unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let api = TaskProductApi::new(
            store.clone(),
            Arc::new(StorageProjectResolver::new(store.clone())),
            AgentRegistry::default_built_ins(),
            Arc::new(SlowPreferencesAgent {
                catalog_on_attach,
                entered: entered_tx,
                release: Mutex::new(release_rx),
            }),
            TaskUpdateNotifier::disabled(),
        )
        .unwrap();
        let created = api
            .create_for_test(TaskAcquireParams {
                project_id: project_id_for_workspace(workspace),
                agent_id: "codex".into(),
                workspace_root: None,
            })
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let pending = api
            .open_for_test(TaskOpenParams {
                task_id: created.task.task_id.clone(),
            })
            .unwrap();
        // Always release the simulated ACP request, including when an assertion fails.
        drop(release_tx);
        assert_eq!(pending.agent_config.state, LiveSessionDataState::Ready);
        assert_eq!(
            protocol_value_id(&pending.agent_config.options[0].current_value),
            Some("agent")
        );
        assert_eq!(
            protocol_value_id(&pending.agent_config.pending_change.unwrap().requested_value),
            Some("agent-full-access")
        );
        assert_ne!(
            pending.send_capability.state,
            TaskSendCapabilityState::Ready
        );
        wait_until(|| {
            api.open_for_test(TaskOpenParams {
                task_id: created.task.task_id.clone(),
            })
            .is_ok_and(|snapshot| snapshot.send_capability.state == TaskSendCapabilityState::Ready)
        });
    }
}

#[derive(Default)]
struct FailingPreferencesAgent {
    fail: AtomicBool,
    calls: AtomicUsize,
}

impl AgentRuntime for FailingPreferencesAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "preferences-session")
            .with_config_options(&mode_config_catalog("agent")))
    }
    fn set_session_config_option(
        &self,
        _: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(RuntimeError::Internal("simulated failure".into()));
        }
        Ok(mode_config_catalog("agent-full-access"))
    }
    fn prompt(
        &self,
        _: AgentPrompt,
        _: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
    }
}

#[test]
fn failed_preferences_require_explicit_retry_or_acceptance_and_check_lease() {
    use openaide_app_server_protocol::snapshot::AgentConfigPreferencesState;
    use openaide_app_server_protocol::task::{
        ConfigPreferencesResolution, TaskResolveConfigPreferencesParams,
    };
    for retry in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let workspace = "/tmp/openaide-unit-workspace/app";
        store.write_task(&task_record("anchor", workspace)).unwrap();
        store
            .write_agent_config_preferences(&mode_config_catalog("agent-full-access"))
            .unwrap();
        let agent = Arc::new(FailingPreferencesAgent {
            fail: AtomicBool::new(true),
            calls: AtomicUsize::new(0),
        });
        let api = TaskProductApi::new(
            store.clone(),
            Arc::new(StorageProjectResolver::new(store.clone())),
            AgentRegistry::default_built_ins(),
            agent.clone(),
            TaskUpdateNotifier::disabled(),
        )
        .unwrap();
        let created = api
            .create_for_test(TaskAcquireParams {
                project_id: project_id_for_workspace(workspace),
                agent_id: "codex".into(),
                workspace_root: None,
            })
            .unwrap();
        let open = || {
            api.open_for_test(TaskOpenParams {
                task_id: created.task.task_id.clone(),
            })
            .unwrap()
        };
        wait_until(|| {
            open()
                .agent_config
                .preferences
                .is_some_and(|state| state.state == AgentConfigPreferencesState::Failed)
        });
        let failed = open();
        assert_eq!(failed.agent_config.state, LiveSessionDataState::Ready);
        assert_eq!(
            protocol_value_id(&failed.agent_config.options[0].current_value),
            Some("agent")
        );
        assert_ne!(failed.send_capability.state, TaskSendCapabilityState::Ready);
        assert!(api
            .send(send_params(created.task.task_id.as_str(), "must not send"))
            .is_err());
        let params = TaskResolveConfigPreferencesParams {
            task_id: created.task.task_id.clone(),
            action: if retry {
                ConfigPreferencesResolution::Retry
            } else {
                ConfigPreferencesResolution::UseCurrentSettings
            },
        };
        assert!(api
            .resolve_config_preferences_for_client(&"different-client".into(), params.clone())
            .is_err());
        agent.fail.store(false, Ordering::SeqCst);
        api.resolve_config_preferences_for_client(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
        .unwrap();
        wait_until(|| open().send_capability.state == TaskSendCapabilityState::Ready);
        assert_eq!(
            protocol_value_id(&open().agent_config.options[0].current_value),
            Some(if retry { "agent-full-access" } else { "agent" })
        );
        assert_eq!(
            agent.calls.load(Ordering::SeqCst),
            if retry { 2 } else { 1 }
        );
        assert_eq!(
            store
                .read_agent_config_preferences("codex")
                .unwrap()
                .options[0]
                .value
                .as_id(),
            Some("agent-full-access")
        );
    }
}

#[test]
fn matching_preferences_make_no_agent_calls_and_unavailable_values_publish_notice() {
    for value in ["agent", "removed-value"] {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let workspace = "/tmp/openaide-unit-workspace/app";
        store.write_task(&task_record("anchor", workspace)).unwrap();
        store
            .write_agent_config_preferences(&mode_config_catalog(value))
            .unwrap();
        let agent = Arc::new(FailingPreferencesAgent::default());
        let api = TaskProductApi::new(
            store.clone(),
            Arc::new(StorageProjectResolver::new(store.clone())),
            AgentRegistry::default_built_ins(),
            agent.clone(),
            TaskUpdateNotifier::disabled(),
        )
        .unwrap();
        let created = api
            .create_for_test(TaskAcquireParams {
                project_id: project_id_for_workspace(workspace),
                agent_id: "codex".into(),
                workspace_root: None,
            })
            .unwrap();
        let open = || {
            api.open_for_test(TaskOpenParams {
                task_id: created.task.task_id.clone(),
            })
            .unwrap()
        };
        wait_until(|| open().send_capability.state == TaskSendCapabilityState::Ready);
        assert_eq!(agent.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            open()
                .agent_config
                .preferences
                .map(|state| state.skipped_count),
            if value == "agent" { None } else { Some(1) }
        );
    }
}
