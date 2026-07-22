#[test]
fn prompt_attachments_are_sent_to_agent_runtime() {
    let tmp = TempDir::new().unwrap();
    let captured = Arc::new(Mutex::new(Vec::<Vec<Attachment>>::new()));
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(AttachmentCapturingAgent {
            prompts: captured.clone(),
        }),
    );

    let created = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Context".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("use file".to_string()),
            external_session_id: None,
            model_id: None,
            context: vec![Attachment {
                kind: "file".to_string(),
                label: "main.rs".to_string(),
                path: Some("/workspace/src/main.rs".to_string()),
                payload: None,
            }],
        })
        .unwrap();
    let task_id = created.task.task_id.clone();

    wait_until(|| captured.lock().unwrap().len() == 1);
    assert_eq!(captured.lock().unwrap()[0][0].label, "main.rs");
    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 100,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    service
        .prompt(SessionPromptParams {
            task_id: task_id.clone(),
            text: "use directory".to_string(),
            prompt_attachments: vec![Attachment {
                kind: "context".to_string(),
                label: "workspace".to_string(),
                path: Some("/workspace".to_string()),
                payload: None,
            }],
            message_id: None,
        })
        .unwrap();

    wait_until(|| captured.lock().unwrap().len() == 2);
    assert_eq!(captured.lock().unwrap()[1][0].label, "workspace");
}

#[test]
fn agent_session_boundary_starts_and_resumes_sessions() {
    let tmp = TempDir::new().unwrap();
    let starts = Arc::new(AtomicUsize::new(0));
    let resumes = Arc::new(AtomicUsize::new(0));
    let prompts = Arc::new(AtomicUsize::new(0));
    let agent = Arc::new(SessionTrackingAgent {
        starts: starts.clone(),
        resumes: resumes.clone(),
        prompts: prompts.clone(),
    });
    let store = Store::open(tmp.path().join("store")).unwrap();
    let service = TaskService::new(store, agent);

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Session boundary".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("first".to_string()),
            external_session_id: None,
            model_id: None,
            context: Vec::new(),
        })
        .unwrap();

    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let task_id = snapshot.task.task_id.clone();
    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 100,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let follow_up = service
        .prompt(SessionPromptParams {
            task_id: task_id.clone(),
            text: "second".to_string(),
            prompt_attachments: Vec::new(),
            message_id: None,
        })
        .unwrap();

    assert_eq!(follow_up.task.status, TaskStatus::Active);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(resumes.load(Ordering::SeqCst), 1);
    wait_until(|| prompts.load(Ordering::SeqCst) == 2);
}

#[test]
fn task_create_adopts_external_session_with_replayed_history() {
    let tmp = TempDir::new().unwrap();
    let loads = Arc::new(AtomicUsize::new(0));
    let resumes = Arc::new(AtomicUsize::new(0));
    let prompts = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(LoadSessionAgent {
            loads: loads.clone(),
            resumes: resumes.clone(),
            prompts: prompts.clone(),
            closes,
        }),
    );

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::AdoptExternalSession,
            title: "".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: None,
            external_session_id: Some("external-session".to_string()),
            model_id: None,
            context: Vec::new(),
        })
        .unwrap();

    assert_eq!(loads.load(Ordering::SeqCst), 1);
    assert_eq!(prompts.load(Ordering::SeqCst), 0);
    assert_eq!(snapshot.task.status, TaskStatus::Inactive);
    assert_eq!(snapshot.task.title, None);
    assert_eq!(
        snapshot.settings_summary.model_id.as_deref(),
        Some("gpt-5.5")
    );
    assert!(snapshot
        .config_options_catalog
        .as_ref()
        .and_then(|catalog| catalog.options.first())
        .is_some_and(|option| option.current_value.as_id() == Some("gpt-5.5")));
    assert_eq!(
        snapshot
            .agent_commands_catalog
            .as_ref()
            .and_then(|catalog| catalog.commands.first())
            .map(|command| command.name.as_str()),
        Some("web")
    );
    assert_eq!(snapshot.chat.items.len(), 2);
    match &snapshot.chat.items[0].message {
        NormalizedMessage::User { text, .. } => assert_eq!(text, "Prior user question"),
        other => panic!("expected replayed user message, got {other:?}"),
    }
    assert_eq!(
        agent_message_text(&snapshot.chat.items[1].message),
        Some("Prior agent answer")
    );

    service
        .prompt(SessionPromptParams {
            task_id: snapshot.task.task_id.clone(),
            text: "Continue loaded session".to_string(),
            prompt_attachments: Vec::new(),
            message_id: None,
        })
        .unwrap();

    assert_eq!(resumes.load(Ordering::SeqCst), 1);
    wait_until(|| prompts.load(Ordering::SeqCst) == 1);
}

#[test]
fn task_create_rejects_duplicate_external_session_adoption() {
    let tmp = TempDir::new().unwrap();
    let loads = Arc::new(AtomicUsize::new(0));
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(LoadSessionAgent {
            loads: loads.clone(),
            resumes: Arc::new(AtomicUsize::new(0)),
            prompts: Arc::new(AtomicUsize::new(0)),
            closes: Arc::new(AtomicUsize::new(0)),
        }),
    );

    let params = || TaskCreateParams {
        mode: TaskCreateMode::AdoptExternalSession,
        title: "".to_string(),
        workspace_root: tmp.path().to_string_lossy().to_string(),
        selected_agent_id: "codex".to_string(),
        selected_agent_label: None,
        selected_isolation: IsolationKind::Local,
        prompt_text: None,
        external_session_id: Some("external-session".to_string()),
        model_id: None,
        context: Vec::new(),
    };

    service.create(params()).unwrap();
    let error = service.create(params()).unwrap_err();

    assert!(matches!(error, RuntimeError::InvalidParams(_)));
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[test]
fn task_delete_deletes_bound_native_session_when_supported() {
    let tmp = TempDir::new().unwrap();
    let deletes = Arc::new(AtomicUsize::new(0));
    let store = Store::open(tmp.path().join("store")).unwrap();
    let service = TaskService::new(
        store.clone(),
        Arc::new(DeleteTrackingLoadSessionAgent {
            deletes: deletes.clone(),
            fail_delete: false,
            store_at_delete: Some(store),
        }),
    );
    let workspace_root = tmp.path().to_string_lossy().to_string();
    let params = || TaskCreateParams {
        mode: TaskCreateMode::AdoptExternalSession,
        title: String::new(),
        workspace_root: workspace_root.clone(),
        selected_agent_id: "codex".to_string(),
        selected_agent_label: Some("Codex".to_string()),
        selected_isolation: IsolationKind::Local,
        prompt_text: None,
        external_session_id: Some("external-session".to_string()),
        model_id: None,
        context: Vec::new(),
    };
    let created = service.create(params()).unwrap();

    service
        .delete(TaskDeleteParams {
            task_id: created.task.task_id.clone(),
            mode: DeleteMode::Delete,
        })
        .unwrap();

    assert_eq!(deletes.load(Ordering::SeqCst), 1);
    service
        .delete(TaskDeleteParams {
            task_id: created.task.task_id,
            mode: DeleteMode::Delete,
        })
        .unwrap();
    assert_eq!(deletes.load(Ordering::SeqCst), 1);
    assert!(matches!(
        service.create(params()).unwrap_err(),
        RuntimeError::InvalidParams(_)
    ));
}

#[test]
fn task_delete_tombstone_blocks_native_session_readoption_when_agent_delete_fails() {
    let tmp = TempDir::new().unwrap();
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(DeleteTrackingLoadSessionAgent {
            deletes: Arc::new(AtomicUsize::new(0)),
            fail_delete: true,
            store_at_delete: None,
        }),
    );
    let workspace_root = tmp.path().to_string_lossy().to_string();
    let params = || TaskCreateParams {
        mode: TaskCreateMode::AdoptExternalSession,
        title: String::new(),
        workspace_root: workspace_root.clone(),
        selected_agent_id: "codex".to_string(),
        selected_agent_label: Some("Codex".to_string()),
        selected_isolation: IsolationKind::Local,
        prompt_text: None,
        external_session_id: Some("external-session".to_string()),
        model_id: None,
        context: Vec::new(),
    };
    let created = service.create(params()).unwrap();

    service
        .delete(TaskDeleteParams {
            task_id: created.task.task_id,
            mode: DeleteMode::Delete,
        })
        .unwrap();

    assert!(matches!(
        service.create(params()).unwrap_err(),
        RuntimeError::InvalidParams(_)
    ));
}

#[cfg(unix)]
#[test]
fn task_create_closes_started_session_when_prompt_start_persistence_fails() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    let store_root = tmp.path().join("store");
    let store = Store::open(store_root.clone()).unwrap();
    let tasks_dir = store_root.join("task-store-v1/tasks");
    let original_permissions = std::fs::metadata(&tasks_dir).unwrap().permissions();
    std::fs::set_permissions(&tasks_dir, std::fs::Permissions::from_mode(0o500)).unwrap();

    let prompts = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let service = TaskService::new(
        store,
        Arc::new(AttachFailingAgent {
            prompts: prompts.clone(),
            closes: closes.clone(),
        }),
    );

    let error = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Persistence failure".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("fail while staging".to_string()),
            external_session_id: None,
            model_id: None,
            context: Vec::new(),
        })
        .unwrap_err();

    std::fs::set_permissions(&tasks_dir, original_permissions).unwrap();
    assert!(matches!(error, RuntimeError::Storage(_)));
    assert_eq!(prompts.load(Ordering::SeqCst), 0);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[test]
fn task_create_closes_loaded_session_when_adoption_persistence_fails() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    let store_root = tmp.path().join("store");
    let store = Store::open(store_root.clone()).unwrap();
    let tasks_dir = store_root.join("task-store-v1/tasks");
    let original_permissions = std::fs::metadata(&tasks_dir).unwrap().permissions();
    std::fs::set_permissions(&tasks_dir, std::fs::Permissions::from_mode(0o500)).unwrap();

    let loads = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let service = TaskService::new(
        store,
        Arc::new(LoadSessionAgent {
            loads: loads.clone(),
            resumes: Arc::new(AtomicUsize::new(0)),
            prompts: Arc::new(AtomicUsize::new(0)),
            closes: closes.clone(),
        }),
    );

    let error = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::AdoptExternalSession,
            title: "".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: None,
            external_session_id: Some("external-session".to_string()),
            model_id: None,
            context: Vec::new(),
        })
        .unwrap_err();

    std::fs::set_permissions(&tasks_dir, original_permissions).unwrap();
    assert!(matches!(error, RuntimeError::Storage(_)));
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}
