#[test]
fn health_uses_jsonrpc_envelope() {
    let (_tmp, mut dispatcher) = dispatcher();

    let response = one_response(
        &mut dispatcher,
        json!({"jsonrpc":"2.0","id":1,"method":"runtime.health"}),
    );

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["status"], "ready");
    assert_eq!(
        response["result"]["methods"],
        json!(["runtime.health", "runtime.shutdown"])
    );
}

#[test]
fn legacy_shell_file_reveal_resolver_is_not_exposed() {
    let (_tmp, mut dispatcher) = dispatcher();

    let response = one_response(
        &mut dispatcher,
        json!({
            "jsonrpc": "2.0",
            "id": "legacy-reveal",
            "method": "shell.fileReveal.resolve",
            "params": { "file_handle_id": "file-reveal-1" }
        }),
    );

    assert_eq!(response["error"]["code"], -32601);
    assert_eq!(response["error"]["data"]["reason"], "method_not_found");
}

#[test]
fn invalid_and_unknown_requests_return_protocol_errors() {
    let (_tmp, mut dispatcher) = dispatcher();

    let parse_error = dispatcher.handle_line("{not json}");
    let parsed: Value = serde_json::from_str(&parse_error[0]).unwrap();
    assert_eq!(parsed["error"]["code"], -32700);

    let unknown = one_response(
        &mut dispatcher,
        json!({"jsonrpc":"2.0","id":"bad","method":"missing.method"}),
    );
    assert_eq!(unknown["error"]["code"], -32601);
}

#[test]
fn blank_task_start_does_not_probe_agent_options() {
    let tmp = TempDir::new().unwrap();
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(MockAgent),
    );

    let error = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("   ".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap_err();

    assert!(matches!(error, RuntimeError::InvalidParams(_)));
}

#[test]
fn active_config_option_updates_mutate_task_settings_summary() {
    let tmp = TempDir::new().unwrap();
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(OptionUpdateAgent),
    );

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Option update".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("switch model".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    let task_id = snapshot.task.task_id;

    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 10,
            })
            .map(|snapshot| {
                snapshot.settings_summary.config_options.get("model")
                    == Some(&"gpt-5.5".to_string())
                    && snapshot.settings_summary.model_id.as_deref() == Some("gpt-5.5")
                    && snapshot
                        .config_options_catalog
                        .as_ref()
                        .and_then(|catalog| catalog.options.first())
                        .is_some_and(|option| option.current_value == "gpt-5.5")
            })
            .unwrap_or(false)
    });
}

#[test]
fn idle_config_option_updates_mutate_task_settings_and_replace_model() {
    let tmp = TempDir::new().unwrap();
    let agent = Arc::new(IdleOptionUpdateAgent::default());
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        agent.clone(),
    );

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Idle option update".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("finish quickly".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    let task_id = snapshot.task.task_id.clone();
    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 10,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    agent.emit_idle_update(mode_only_catalog("plan"));

    let snapshot = service
        .snapshot(TaskSnapshotParams {
            task_id,
            tail_limit: 10,
        })
        .unwrap();
    assert_eq!(
        snapshot.settings_summary.config_options.get("mode"),
        Some(&"plan".to_string())
    );
    assert_eq!(snapshot.settings_summary.model_id, None);
}

#[test]
fn idle_agent_title_updates_persist_and_clear_the_agent_owned_title() {
    let tmp = TempDir::new().unwrap();
    let agent = Arc::new(IdleOptionUpdateAgent::default());
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        agent.clone(),
    );
    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Local fallback".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("finish quickly".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    let task_id = snapshot.task.task_id.clone();
    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 10,
            })
            .is_ok_and(|snapshot| snapshot.task.status == TaskStatus::Inactive)
    });

    agent.emit_metadata_update(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Value("Agent generated title".to_string()),
        updated_at: AgentMetadataField::Unchanged,
    });
    assert_eq!(
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 10,
            })
            .unwrap()
            .task
            .title
            .as_ref()
            .map(|title| title.value()),
        Some("Agent generated title")
    );

    agent.emit_metadata_update(AgentSessionMetadataUpdate {
        title: AgentMetadataField::Clear,
        updated_at: AgentMetadataField::Unchanged,
    });
    assert_eq!(
        service
            .snapshot(TaskSnapshotParams {
                task_id,
                tail_limit: 10,
            })
            .unwrap()
            .task
            .title,
        None
    );
}
