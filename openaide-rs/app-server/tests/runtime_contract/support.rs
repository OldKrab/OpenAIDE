fn dispatcher() -> (TempDir, ShellControlDispatcher) {
    let tmp = TempDir::new().unwrap();
    let runtime = Runtime::new_with_agent(tmp.path().join("store"), Arc::new(MockAgent)).unwrap();
    (tmp, ShellControlDispatcher::new(runtime))
}

fn one_response(dispatcher: &mut ShellControlDispatcher, request: Value) -> Value {
    let line = serde_json::to_string(&request).unwrap();
    let responses = dispatcher.handle_line(&line);
    assert_eq!(responses.len(), 1);
    serde_json::from_str(&responses[0]).unwrap()
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for predicate");
        thread::sleep(Duration::from_millis(10));
    }
}

fn has_running_activity(snapshot: &TaskSnapshot) -> bool {
    snapshot.chat.items.iter().any(|item| match &item.message {
        openaide_app_server::protocol::model::NormalizedMessage::Activity { status, .. } => {
            *status == ActivityStatus::Running
        }
        _ => false,
    })
}

fn has_interruption_reason(
    snapshot: &TaskSnapshot,
    predicate: impl Fn(InterruptionReason) -> bool,
) -> bool {
    snapshot.chat.items.iter().any(|item| match &item.message {
        NormalizedMessage::Interruption { reason, .. } => predicate(*reason),
        _ => false,
    })
}

struct CountingAgent {
    calls: Arc<AtomicUsize>,
}

impl AgentRuntime for CountingAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_counting"))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        thread::sleep(Duration::from_millis(50));
        if prompt.cancellation.is_cancelled() {
            return Ok(());
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        sink.emit(AgentEvent::Text("counted response".to_string()))
    }
}

#[derive(Default)]
struct PassiveSnapshotPromptState {
    parked: bool,
    released: bool,
}

struct PassiveSnapshotAgent {
    calls: Arc<AtomicUsize>,
    state: Arc<(Mutex<PassiveSnapshotPromptState>, Condvar)>,
}

impl AgentRuntime for PassiveSnapshotAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_passive_snapshot"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let (state_lock, changed) = &*self.state;
        let mut state = state_lock.lock().unwrap();
        state.parked = true;
        changed.notify_all();
        while !state.released {
            state = changed.wait(state).unwrap();
        }
        sink.emit(AgentEvent::Text("counted response".to_string()))
    }
}

struct WaitingAgent {
    started: Arc<AtomicUsize>,
    cancelled: Arc<AtomicUsize>,
}

impl AgentRuntime for WaitingAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_waiting"))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !prompt.cancellation.is_cancelled() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if prompt.cancellation.is_cancelled() {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        }
        sink.emit(AgentEvent::Text("should not be stored".to_string()))
    }
}

struct SessionTrackingAgent {
    starts: Arc<AtomicUsize>,
    resumes: Arc<AtomicUsize>,
    prompts: Arc<AtomicUsize>,
}

impl AgentRuntime for SessionTrackingAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(format!(
            "session_for_{}",
            request.task_id
        )))
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        self.resumes.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(request.session_id))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        assert!(!prompt.session_id.is_empty());
        self.prompts.fetch_add(1, Ordering::SeqCst);
        sink.emit(AgentEvent::Text("tracked response".to_string()))
    }
}

struct LoadSessionAgent {
    loads: Arc<AtomicUsize>,
    resumes: Arc<AtomicUsize>,
    prompts: Arc<AtomicUsize>,
    closes: Arc<AtomicUsize>,
}

impl AgentRuntime for LoadSessionAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(
            "prompt start is not used by adoption".to_string(),
        ))
    }

    fn load_session(&self, request: AgentSessionLoad) -> Result<AgentLoadedSession, RuntimeError> {
        assert_eq!(request.session_id, "external-session");
        assert!(std::path::Path::new(&request.cwd).is_absolute());
        assert!(!request.cancellation.is_cancelled());
        self.loads.fetch_add(1, Ordering::SeqCst);

        let mut session = AgentSession::new(request.session_id);
        session
            .config_options
            .insert("model".to_string(), "gpt-5.5".to_string());
        session.model_id = Some("gpt-5.5".to_string());
        session.commands_catalog = Some(AgentCommandsCatalog {
            commands: vec![AgentCommand {
                name: "web".to_string(),
                description: "Search the web".to_string(),
                input_hint: Some("query".to_string()),
            }],
        });
        Ok(AgentLoadedSession {
            session,
            replayed_messages: vec![
                NormalizedMessage::User {
                    id: "replayed-user".to_string(),
                    text: "Prior user question".to_string(),
                    created_at: "2026-05-18T00:00:00Z".to_string(),
                    attachments: Vec::new(),
                },
                NormalizedMessage::AgentText {
                    id: "replayed-agent".to_string(),
                    text: "Prior agent answer".to_string(),
                    created_at: "2026-05-18T00:00:01Z".to_string(),
                    streaming: false,
                },
            ],
        })
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        assert_eq!(request.session_id, "external-session");
        self.resumes.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(request.session_id))
    }

    fn close_session(&self, session_id: &str) -> Result<(), RuntimeError> {
        assert_eq!(session_id, "external-session");
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        assert_eq!(prompt.session_id, "external-session");
        self.prompts.fetch_add(1, Ordering::SeqCst);
        sink.emit(AgentEvent::Text("continued loaded session".to_string()))
    }
}

struct DeleteTrackingLoadSessionAgent {
    deletes: Arc<AtomicUsize>,
    fail_delete: bool,
    store_at_delete: Option<Store>,
}

impl AgentRuntime for DeleteTrackingLoadSessionAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(
            "prompt start is not used by adoption".to_string(),
        ))
    }

    fn load_session(&self, request: AgentSessionLoad) -> Result<AgentLoadedSession, RuntimeError> {
        assert_eq!(request.session_id, "external-session");
        Ok(AgentLoadedSession {
            session: AgentSession::new(request.session_id),
            replayed_messages: Vec::new(),
        })
    }

    fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        assert_eq!(request.session_id, "external-session");
        if let Some(store) = &self.store_at_delete {
            let task = store
                .list_all_task_records()
                .unwrap()
                .into_iter()
                .find(|task| task.agent_session_id.as_deref() == Some("external-session"))
                .expect("bound task should exist when native delete runs");
            assert!(task.tombstoned, "native delete must run after tombstone commit");
        }
        self.deletes.fetch_add(1, Ordering::SeqCst);
        if self.fail_delete {
            Err(RuntimeError::NotReady("delete failed".to_string()))
        } else {
            Ok(())
        }
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}

struct DelayedAgent;

impl AgentRuntime for DelayedAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_delayed"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        thread::sleep(Duration::from_millis(80));
        sink.emit(AgentEvent::Text("delayed response".to_string()))
    }
}

struct OptionsCountingAgent {
    calls: Arc<AtomicUsize>,
}

impl AgentRuntime for OptionsCountingAgent {
    fn config_options(
        &self,
        _request: AgentConfigOptionsRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(model_catalog("gpt-5.4"))
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_options_counting"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        sink.emit(AgentEvent::Text("response".to_string()))
    }
}

struct OptionUpdateAgent;

impl AgentRuntime for OptionUpdateAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_option_update")
            .with_config_options(&model_catalog("gpt-5.4")))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        sink.emit(AgentEvent::ConfigOptionsChanged(model_catalog("gpt-5.5")))?;
        sink.emit(AgentEvent::Text("updated".to_string()))
    }
}

struct ToolCallUpdateAgent;

impl AgentRuntime for ToolCallUpdateAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_tool_call_update"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        sink.emit(AgentEvent::ToolCall(AgentToolCall {
            tool_call_id: "tool_call_1".to_string(),
            scope_id: None,
            title: "Read configuration".to_string(),
            kind: "read".to_string(),
            status: AgentToolCallStatus::InProgress,
            input_summary: Some("config.toml".to_string()),
            output_preview: None,
            details: None,
        }))?;
        sink.emit(AgentEvent::ToolCall(AgentToolCall {
            tool_call_id: "tool_call_1".to_string(),
            scope_id: None,
            title: "Read configuration".to_string(),
            kind: "read".to_string(),
            status: AgentToolCallStatus::Completed,
            input_summary: Some("config.toml".to_string()),
            output_preview: Some("Found configuration".to_string()),
            details: None,
        }))?;
        sink.emit(AgentEvent::Text("done".to_string()))
    }
}

struct ChunkedTextAgent;

impl AgentRuntime for ChunkedTextAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_chunked_text"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        for chunk in ["I will ", "run ", "`pwd`."] {
            sink.emit(AgentEvent::Text(chunk.to_string()))?;
        }
        sink.emit(AgentEvent::ToolCall(AgentToolCall {
            tool_call_id: "tool_call_pwd".to_string(),
            scope_id: None,
            title: "pwd".to_string(),
            kind: "execute".to_string(),
            status: AgentToolCallStatus::Completed,
            input_summary: Some("pwd".to_string()),
            output_preview: Some("/home/user".to_string()),
            details: None,
        }))?;
        for chunk in ["Called", " `", "pwd", "`:", " `/", "home", "/us", "er", "`"] {
            sink.emit(AgentEvent::Text(chunk.to_string()))?;
        }
        Ok(())
    }
}

struct PermissionBoundaryTextAgent;

impl AgentRuntime for PermissionBoundaryTextAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_permission_boundary_text"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        for chunk in ["Need ", "approval."] {
            sink.emit(AgentEvent::Text(chunk.to_string()))?;
        }
        let _outcome = sink.request_permission(AgentPermissionRequest {
            request_id: "permission_boundary".to_string(),
            title: "Allow follow-up".to_string(),
            description: Some("Continue after approval.".to_string()),
            scope: Some("workspace".to_string()),
            risk: None,
            tool_call: AgentToolCallRef {
                tool_call_id: "tool_permission_boundary".to_string(),
                title: "Allow follow-up".to_string(),
                kind: Some("edit".to_string()),
            },
            options: vec![AgentPermissionOption {
                option_id: "allow".to_string(),
                name: "Allow".to_string(),
                kind: AgentPermissionOptionKind::AllowOnce,
            }],
        })?;
        for chunk in ["After ", "approval."] {
            sink.emit(AgentEvent::Text(chunk.to_string()))?;
        }
        Ok(())
    }
}

struct AttachmentCapturingAgent {
    prompts: Arc<Mutex<Vec<Vec<Attachment>>>>,
}

impl AgentRuntime for AttachmentCapturingAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_attachment_capture"))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        self.prompts.lock().unwrap().push(prompt.attachments);
        sink.emit(AgentEvent::Text("captured attachments".to_string()))
    }
}

#[derive(Default)]
struct IdleOptionUpdateAgent {
    sink: Mutex<Option<Arc<dyn AgentSessionEventSink>>>,
}

impl IdleOptionUpdateAgent {
    fn emit_idle_update(&self, catalog: ConfigOptionsCatalog) {
        let sink = self
            .sink
            .lock()
            .unwrap()
            .clone()
            .expect("session event sink attached");
        sink.config_options_changed(catalog).unwrap();
    }

    fn emit_metadata_update(&self, update: AgentSessionMetadataUpdate) {
        let sink = self
            .sink
            .lock()
            .unwrap()
            .clone()
            .expect("session event sink attached");
        sink.metadata_changed(update).unwrap();
    }
}

impl AgentRuntime for IdleOptionUpdateAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_idle_option_update")
            .with_config_options(&model_catalog("gpt-5.4")))
    }

    fn attach_session_event_sink(
        &self,
        _session_id: &str,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        *self.sink.lock().unwrap() = Some(sink);
        Ok(())
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        sink.emit(AgentEvent::Text("done".to_string()))
    }
}

struct ShutdownTrackingAgent {
    closes: Arc<AtomicUsize>,
}

impl AgentRuntime for ShutdownTrackingAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_shutdown_tracking"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        sink.emit(AgentEvent::Text("response".to_string()))
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Default)]
struct ShutdownPromptState {
    prompt_started: bool,
    prompt_returned: bool,
    closed: bool,
}

struct ShutdownBlockingAgent {
    state: Arc<(Mutex<ShutdownPromptState>, Condvar)>,
}

impl AgentRuntime for ShutdownBlockingAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_shutdown_blocking"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        let (state_lock, changed) = &*self.state;
        let mut state = state_lock.lock().unwrap();
        state.prompt_started = true;
        changed.notify_all();
        while !state.closed {
            state = changed.wait(state).unwrap();
        }
        state.prompt_returned = true;
        changed.notify_all();
        Err(RuntimeError::NotReady("ACP session closed".to_string()))
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        let (state_lock, changed) = &*self.state;
        let mut state = state_lock.lock().unwrap();
        state.closed = true;
        changed.notify_all();
        Ok(())
    }
}

struct AttachFailingAgent {
    prompts: Arc<AtomicUsize>,
    closes: Arc<AtomicUsize>,
}

impl AgentRuntime for AttachFailingAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new("session_attach_failing"))
    }

    fn attach_session_event_sink(
        &self,
        _session_id: &str,
        _sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::NotReady("session worker stopped".to_string()))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn close_session(&self, _session_id: &str) -> Result<(), RuntimeError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct FollowupAttachFailingAgent {
    starts: Arc<AtomicUsize>,
    resumes: Arc<AtomicUsize>,
    prompts: Arc<AtomicUsize>,
    closes: Arc<AtomicUsize>,
    attach_calls: Arc<AtomicUsize>,
}

impl AgentRuntime for FollowupAttachFailingAgent {
    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        let idx = self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(format!("session_followup_{idx}")))
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        self.resumes.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(request.session_id))
    }

    fn attach_session_event_sink(
        &self,
        _session_id: &str,
        _sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        let call = self.attach_calls.fetch_add(1, Ordering::SeqCst);
        if call == 1 {
            Err(RuntimeError::NotReady("session worker stopped".to_string()))
        } else {
            Ok(())
        }
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        sink.emit(AgentEvent::Text("follow-up response".to_string()))
    }

    fn close_session(&self, _session_id: &str) -> Result<(), RuntimeError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn model_catalog(current_model: &str) -> ConfigOptionsCatalog {
    ConfigOptionsCatalog {
        agent_id: "codex".to_string(),
        status: ConfigOptionsStatus::Ready,
        options: vec![ConfigOption {
            id: "model".to_string(),
            label: "Model".to_string(),
            description: Some("Model selector".to_string()),
            category: Some(ConfigOptionCategory::Model),
            current_value: current_model.to_string(),
            values: vec![
                ConfigOptionValue {
                    id: "gpt-5.4".to_string(),
                    label: "gpt-5.4".to_string(),
                    description: None,
                    group_id: None,
                    group_label: None,
                },
                ConfigOptionValue {
                    id: "gpt-5.5".to_string(),
                    label: "gpt-5.5".to_string(),
                    description: None,
                    group_id: None,
                    group_label: None,
                },
            ],
        }],
    }
}

fn mode_only_catalog(current_mode: &str) -> ConfigOptionsCatalog {
    ConfigOptionsCatalog {
        agent_id: "codex".to_string(),
        status: ConfigOptionsStatus::Ready,
        options: vec![ConfigOption {
            id: "mode".to_string(),
            label: "Mode".to_string(),
            description: None,
            category: Some(ConfigOptionCategory::Mode),
            current_value: current_mode.to_string(),
            values: vec![
                ConfigOptionValue {
                    id: "plan".to_string(),
                    label: "Plan".to_string(),
                    description: None,
                    group_id: None,
                    group_label: None,
                },
                ConfigOptionValue {
                    id: "code".to_string(),
                    label: "Code".to_string(),
                    description: None,
                    group_id: None,
                    group_label: None,
                },
            ],
        }],
    }
}
