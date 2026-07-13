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
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "session_counting"))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        // Keep the prompt active until the cancellation path under test reaches it.
        let deadline = Instant::now() + Duration::from_secs(2);
        while !prompt.cancellation.is_cancelled() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if prompt.cancellation.is_cancelled() {
            return Ok(openaide_app_server::agent::AgentPromptOutcome::Cancelled);
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        sink.emit(agent_text_event("counted response"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
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
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "session_passive_snapshot",
        ))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let (state_lock, changed) = &*self.state;
        let mut state = state_lock.lock().unwrap();
        state.parked = true;
        changed.notify_all();
        while !state.released {
            state = changed.wait(state).unwrap();
        }
        sink.emit(agent_text_event("counted response"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }
}

struct WaitingAgent {
    started: Arc<AtomicUsize>,
    cancelled: Arc<AtomicUsize>,
}

impl AgentRuntime for WaitingAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "session_waiting"))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !prompt.cancellation.is_cancelled() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if prompt.cancellation.is_cancelled() {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        }
        sink.emit(agent_text_event("should not be stored"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::Cancelled)
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
        Ok(AgentSession::new(
            request.agent_id,
            format!("session_for_{}", request.task_id),
        ))
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        self.resumes.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(request.agent_id, request.session_id))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        assert!(!prompt.session_id.is_empty());
        self.prompts.fetch_add(1, Ordering::SeqCst);
        sink.emit(agent_text_event("tracked response"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
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

        let mut session = AgentSession::new(request.agent_id, request.session_id);
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
                normalized_agent_text(
                    "replayed-agent",
                    "Prior agent answer",
                    "2026-05-18T00:00:01Z",
                ),
            ],
        })
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        assert_eq!(request.session_id, "external-session");
        self.resumes.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(request.agent_id, request.session_id))
    }

    fn close_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        assert_eq!(session.session_id(), "external-session");
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        assert_eq!(prompt.session_id, "external-session");
        self.prompts.fetch_add(1, Ordering::SeqCst);
        sink.emit(agent_text_event("continued loaded session"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
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
            session: AgentSession::new(request.agent_id, request.session_id),
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
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }
}

struct DelayedAgent;

impl AgentRuntime for DelayedAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "session_delayed"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        thread::sleep(Duration::from_millis(80));
        sink.emit(agent_text_event("delayed response"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }
}

struct OptionUpdateAgent;

impl AgentRuntime for OptionUpdateAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "session_option_update")
            .with_config_options(&model_catalog("gpt-5.4")))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        sink.emit(AgentEvent::ConfigOptionsChanged(model_catalog("gpt-5.5")))?;
        sink.emit(agent_text_event("updated"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }
}

struct ToolCallUpdateAgent;

impl AgentRuntime for ToolCallUpdateAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "session_tool_call_update",
        ))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
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
        sink.emit(agent_text_event("done"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }
}

struct ChunkedTextAgent;

impl AgentRuntime for ChunkedTextAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "session_chunked_text"))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        for chunk in ["I will ", "run ", "`pwd`."] {
            sink.emit(agent_text_event(chunk))?;
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
            sink.emit(agent_text_event(chunk))?;
        }
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }
}

struct MessageIdSpanningToolAgent;

impl AgentRuntime for MessageIdSpanningToolAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "session_message_id_spanning_tool",
        ))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        sink.emit(sourced_agent_text_event("Before ", "message-1"))?;
        sink.emit(AgentEvent::ToolCall(AgentToolCall {
            tool_call_id: "tool_between_chunks".to_string(),
            scope_id: None,
            title: "Tool between chunks".to_string(),
            kind: "execute".to_string(),
            status: AgentToolCallStatus::Completed,
            input_summary: None,
            output_preview: None,
            details: None,
        }))?;
        sink.emit(sourced_agent_text_event("after.", "message-1"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }
}

struct AttachmentCapturingAgent {
    prompts: Arc<Mutex<Vec<Vec<Attachment>>>>,
}

impl AgentRuntime for AttachmentCapturingAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "session_attachment_capture",
        ))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        self.prompts.lock().unwrap().push(prompt.attachments);
        sink.emit(agent_text_event("captured attachments"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
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
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, "session_idle_option_update")
            .with_config_options(&model_catalog("gpt-5.4")))
    }

    fn attach_session_event_sink(
        &self,
        _session: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        *self.sink.lock().unwrap() = Some(sink);
        Ok(())
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        sink.emit(agent_text_event("done"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }
}

struct ShutdownTrackingAgent {
    closes: Arc<AtomicUsize>,
}

impl AgentRuntime for ShutdownTrackingAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "session_shutdown_tracking",
        ))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        sink.emit(agent_text_event("response"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
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
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "session_shutdown_blocking",
        ))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
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
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "session_attach_failing",
        ))
    }

    fn attach_session_event_sink(
        &self,
        _session: &AgentSessionKey,
        _sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::NotReady("session worker stopped".to_string()))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }

    fn close_session(&self, _session: &AgentSessionKey) -> Result<(), RuntimeError> {
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
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        let idx = self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(
            request.agent_id,
            format!("session_followup_{idx}"),
        ))
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        self.resumes.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(request.agent_id, request.session_id))
    }

    fn attach_session_event_sink(
        &self,
        _session: &AgentSessionKey,
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
    ) -> Result<openaide_app_server::agent::AgentPromptOutcome, RuntimeError> {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        sink.emit(agent_text_event("follow-up response"))?;
        Ok(openaide_app_server::agent::AgentPromptOutcome::EndTurn)
    }

    fn close_session(&self, _session: &AgentSessionKey) -> Result<(), RuntimeError> {
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
