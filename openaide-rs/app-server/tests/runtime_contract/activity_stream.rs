#[test]
fn tool_call_updates_replace_existing_activity_by_identity() {
    let tmp = TempDir::new().unwrap();
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(ToolCallUpdateAgent),
    );

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Tool call update".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("read config".to_string()),
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
                tail_limit: 100,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let snapshot = service
        .snapshot(TaskSnapshotParams {
            task_id: task_id.clone(),
            tail_limit: 100,
        })
        .unwrap();
    let tool_messages = snapshot
        .chat
        .items
        .iter()
        .filter(|item| {
            matches!(
                &item.message,
                NormalizedMessage::Activity { title, .. } if title == "Read configuration"
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(tool_messages.len(), 1);
    assert!(tool_messages[0].identity.starts_with("acp_tool:"));
    assert!(tool_messages[0].identity.ends_with(":tool_call_1"));
    match &tool_messages[0].message {
        NormalizedMessage::Activity {
            id,
            title,
            status,
            steps,
            ..
        } => {
            assert_eq!(id, &tool_messages[0].identity);
            assert_eq!(title, "Read configuration");
            assert_eq!(*status, ActivityStatus::Completed);
            assert_eq!(steps.len(), 1);
            match &steps[0] {
                openaide_app_server::protocol::model::ActivityStep::Tool {
                    tool_call_id,
                    name,
                    status,
                    input_summary,
                    output_preview,
                    details,
                    ..
                } => {
                    assert_eq!(tool_call_id.as_deref(), Some("tool_call_1"));
                    assert_eq!(name, "read");
                    assert_eq!(*status, ActivityStatus::Completed);
                    assert_eq!(input_summary.as_deref(), Some("config.toml"));
                    assert_eq!(output_preview.as_deref(), Some("Found configuration"));
                    assert!(details.is_none());
                }
                other => panic!("expected tool step, got {other:?}"),
            }
        }
        other => panic!("expected activity, got {other:?}"),
    }

    service
        .prompt(SessionPromptParams {
            task_id: task_id.clone(),
            text: "read config again".to_string(),
            prompt_attachments: Vec::new(),
            message_id: None,
        })
        .unwrap();

    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 100,
            })
            .map(|snapshot| {
                snapshot.task.status == TaskStatus::Inactive
                    && snapshot
                        .chat
                        .items
                        .iter()
                        .filter(|item| {
                            matches!(
                                &item.message,
                                NormalizedMessage::Activity { title, .. }
                                    if title == "Read configuration"
                            )
                        })
                        .count()
                        == 2
            })
            .unwrap_or(false)
    });

    let snapshot = service
        .snapshot(TaskSnapshotParams {
            task_id,
            tail_limit: 100,
        })
        .unwrap();
    let identities = snapshot
        .chat
        .items
        .iter()
        .filter_map(|item| match &item.message {
            NormalizedMessage::Activity { title, .. } if title == "Read configuration" => {
                Some(item.identity.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(identities.len(), 2);
    assert_ne!(identities[0], identities[1]);
    assert!(identities.iter().all(|id| id.ends_with(":tool_call_1")));
}

#[test]
fn streamed_agent_text_chunks_are_persisted_as_one_message_per_contiguous_run() {
    let tmp = TempDir::new().unwrap();
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(ChunkedTextAgent),
    );

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Chunked text".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("call any tool".to_string()),
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
                tail_limit: 100,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let snapshot = service
        .snapshot(TaskSnapshotParams {
            task_id,
            tail_limit: 100,
        })
        .unwrap();
    let agent_texts = snapshot
        .chat
        .items
        .iter()
        .filter_map(|item| match &item.message {
            NormalizedMessage::AgentText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        agent_texts,
        vec!["I will run `pwd`.", "Called `pwd`: `/home/user`"]
    );
}

#[test]
fn permission_requests_split_streamed_agent_text_runs() {
    let tmp = TempDir::new().unwrap();
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(PermissionBoundaryTextAgent),
    );

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Permission boundary".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("ask permission".to_string()),
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
                tail_limit: 100,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Blocked)
            .unwrap_or(false)
    });

    let blocked = service
        .snapshot(TaskSnapshotParams {
            task_id: task_id.clone(),
            tail_limit: 100,
        })
        .unwrap();
    let (request_id, option_id) = blocked
        .chat
        .items
        .iter()
        .find_map(|item| match &item.message {
            NormalizedMessage::Permission {
                request_id,
                options,
                ..
            } => Some((request_id.clone(), options[0].id.clone())),
            _ => None,
        })
        .expect("snapshot contains permission request");

    service
        .respond_permission(PermissionRespondParams {
            task_id: task_id.clone(),
            request_id,
            decision: PermissionDecision::Approved,
            option_id,
        })
        .unwrap();

    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 100,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let snapshot = service
        .snapshot(TaskSnapshotParams {
            task_id,
            tail_limit: 100,
        })
        .unwrap();
    let agent_texts = snapshot
        .chat
        .items
        .iter()
        .filter_map(|item| match &item.message {
            NormalizedMessage::AgentText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(agent_texts, vec!["Need approval.", "After approval."]);
}
