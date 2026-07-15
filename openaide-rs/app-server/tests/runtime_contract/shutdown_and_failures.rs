#[test]
fn runtime_shutdown_closes_agent_sessions() {
    let tmp = TempDir::new().unwrap();
    let closes = Arc::new(AtomicUsize::new(0));
    let runtime = Runtime::new_with_agent(
        tmp.path().join("store"),
        Arc::new(ShutdownTrackingAgent {
            closes: closes.clone(),
        }),
    )
    .unwrap();
    let mut dispatcher = ShellControlDispatcher::new(runtime);

    let response = one_response(
        &mut dispatcher,
        json!({"jsonrpc":"2.0","id":"shutdown","method":"runtime.shutdown","params":{}}),
    );

    assert_eq!(response["result"], json!({}));
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}

#[test]
fn runtime_shutdown_acknowledges_and_sets_stop_flag() {
    let (_tmp, mut dispatcher) = dispatcher();

    let response = one_response(
        &mut dispatcher,
        json!({"jsonrpc":"2.0","id":"shutdown","method":"runtime.shutdown","params":{}}),
    );

    assert_eq!(response["result"], json!({}));
    assert!(dispatcher.shutdown_requested());
}

#[test]
fn shutdown_preserves_durable_native_session_bindings_before_restart() {
    let tmp = TempDir::new().unwrap();
    let store_root = tmp.path().join("store");
    let starts = Arc::new(AtomicUsize::new(0));
    let resumes = Arc::new(AtomicUsize::new(0));
    let prompts = Arc::new(AtomicUsize::new(0));
    let store = Store::open(store_root.clone()).unwrap();
    let service = TaskService::new(
        store.clone(),
        Arc::new(SessionTrackingAgent {
            starts: starts.clone(),
            resumes,
            prompts,
        }),
    );

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Shutdown clears session".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("first".to_string()),
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
    assert!(store
        .read_task(&task_id)
        .unwrap()
        .agent_session_id
        .is_some());

    service.shutdown().unwrap();

    assert!(store
        .read_task(&task_id)
        .unwrap()
        .agent_session_id
        .is_some());
    drop(service);
    drop(store);

    let restart_starts = Arc::new(AtomicUsize::new(0));
    let restart_resumes = Arc::new(AtomicUsize::new(0));
    let restart_prompts = Arc::new(AtomicUsize::new(0));
    let restarted = TaskService::new(
        Store::open(store_root).unwrap(),
        Arc::new(SessionTrackingAgent {
            starts: restart_starts.clone(),
            resumes: restart_resumes.clone(),
            prompts: restart_prompts.clone(),
        }),
    );
    restarted
        .prompt(SessionPromptParams {
            task_id: task_id.clone(),
            text: "after restart".to_string(),
            prompt_attachments: Vec::new(),
            message_id: None,
        })
        .unwrap();

    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(restart_starts.load(Ordering::SeqCst), 0);
    assert_eq!(restart_resumes.load(Ordering::SeqCst), 1);
    wait_until(|| restart_prompts.load(Ordering::SeqCst) == 1);
}

#[test]
fn runtime_startup_recovers_stale_active_turn_and_session_binding() {
    let tmp = TempDir::new().unwrap();
    let store_root = tmp.path().join("store");
    let store = Store::open(store_root.clone()).unwrap();
    let task_id = "task_stale_boot".to_string();
    store
        .write_task(&TaskRecord {
            task_id: task_id.clone(),
            title: openaide_app_server::storage::records::TaskTitle::new(
                "Stale boot",
                openaide_app_server::storage::records::TaskTitleSource::User,
            ),
            status: TaskStatus::Active,
            task_version: 1,
            message_history_version: 0,
            unread: false,
            attention: None,
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
            last_activity: "1".to_string(),
            agent_name: "Codex".to_string(),
            agent_id: "codex".to_string(),
            isolation: IsolationKind::Local,
            workspace_root: tmp.path().to_string_lossy().to_string(),
            lifecycle: openaide_app_server::storage::records::TaskLifecycle::Visible,
            agent_session_id: Some("session_stale_boot".to_string()),
            active_turn_id: Some("turn_stale_boot".to_string()),
            archived: false,
            tombstoned: false,
            revision: 1,
            config_options: Default::default(),
            config_options_catalog: None,
            config_mutation: Default::default(),
            agent_commands_catalog: None,
            model_id: None,
            preparation: TaskPreparationRecord::Ready,
        })
        .unwrap();
    drop(store);

    let starts = Arc::new(AtomicUsize::new(0));
    let resumes = Arc::new(AtomicUsize::new(0));
    let prompts = Arc::new(AtomicUsize::new(0));
    let runtime = Runtime::new_with_agent(
        store_root.clone(),
        Arc::new(SessionTrackingAgent {
            starts: starts.clone(),
            resumes: resumes.clone(),
            prompts: prompts.clone(),
        }),
    )
    .unwrap();

    let snapshot = runtime
        .service()
        .snapshot(TaskSnapshotParams {
            task_id: task_id.clone(),
            tail_limit: 100,
        })
        .unwrap();
    assert_eq!(snapshot.task.status, TaskStatus::Inactive);
    assert!(has_interruption_reason(&snapshot, |reason| {
        matches!(reason, InterruptionReason::BackendUnavailable)
    }));

    runtime.service().shutdown().unwrap();
    drop(runtime);

    let recovered_store = Store::open(store_root.clone()).unwrap();
    let recovered = recovered_store.read_task(&task_id).unwrap();
    assert_eq!(recovered.status, TaskStatus::Inactive);
    assert!(recovered.active_turn_id.is_none());
    assert_eq!(
        recovered.agent_session_id.as_deref(),
        Some("session_stale_boot")
    );
    recovered_store.mark_clean_shutdown().unwrap();
    drop(recovered_store);

    let restarted = Runtime::new_with_agent(
        store_root,
        Arc::new(SessionTrackingAgent {
            starts: starts.clone(),
            resumes: resumes.clone(),
            prompts: prompts.clone(),
        }),
    )
    .unwrap();
    restarted
        .service()
        .prompt(SessionPromptParams {
            task_id: task_id.clone(),
            text: "after recovery".to_string(),
            prompt_attachments: Vec::new(),
            message_id: None,
        })
        .unwrap();

    assert_eq!(starts.load(Ordering::SeqCst), 0);
    assert_eq!(resumes.load(Ordering::SeqCst), 1);
    wait_until(|| prompts.load(Ordering::SeqCst) == 1);
}

#[test]
fn shutdown_stops_active_turn_without_failed_task_state() {
    let tmp = TempDir::new().unwrap();
    let state = Arc::new((Mutex::new(ShutdownPromptState::default()), Condvar::new()));
    let service = TaskService::new(
        Store::open(tmp.path().join("store")).unwrap(),
        Arc::new(ShutdownBlockingAgent {
            state: state.clone(),
        }),
    );

    let created = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Shutdown active turn".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("block until shutdown".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    let task_id = created.task.task_id.clone();
    wait_until(|| state.0.lock().unwrap().prompt_started);

    service.shutdown().unwrap();
    wait_until(|| state.0.lock().unwrap().prompt_returned);
    thread::sleep(Duration::from_millis(25));

    let snapshot = service
        .snapshot(TaskSnapshotParams {
            task_id,
            tail_limit: 100,
        })
        .unwrap();
    assert_eq!(snapshot.task.status, TaskStatus::Inactive);
    assert!(has_interruption_reason(&snapshot, |reason| {
        matches!(reason, InterruptionReason::Canceled)
    }));
    assert!(!has_interruption_reason(&snapshot, |reason| {
        matches!(reason, InterruptionReason::Failed)
    }));
    assert!(!has_running_activity(&snapshot));
}

#[test]
fn task_create_attach_failure_finalizes_created_task() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("store")).unwrap();
    let prompts = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let service = TaskService::new(
        store.clone(),
        Arc::new(AttachFailingAgent {
            prompts: prompts.clone(),
            closes: closes.clone(),
        }),
    );

    let error = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Attach failure".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("will not run".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap_err();

    assert!(matches!(error, RuntimeError::NotReady(_)));
    assert_eq!(prompts.load(Ordering::SeqCst), 0);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
    let records = store.list_tasks().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, TaskStatus::Inactive);
    assert!(records[0].active_turn_id.is_none());
    assert!(records[0].agent_session_id.is_none());

    let snapshot = service
        .snapshot(TaskSnapshotParams {
            task_id: records[0].task_id.clone(),
            tail_limit: 100,
        })
        .unwrap();
    assert!(has_interruption_reason(&snapshot, |reason| {
        matches!(reason, InterruptionReason::Failed)
    }));
    assert!(!has_running_activity(&snapshot));
}

#[test]
fn follow_up_attach_failure_preserves_resumed_native_session() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("store")).unwrap();
    let starts = Arc::new(AtomicUsize::new(0));
    let resumes = Arc::new(AtomicUsize::new(0));
    let prompts = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let attach_calls = Arc::new(AtomicUsize::new(0));
    let service = TaskService::new(
        store.clone(),
        Arc::new(FollowupAttachFailingAgent {
            starts: starts.clone(),
            resumes: resumes.clone(),
            prompts: prompts.clone(),
            closes: closes.clone(),
            attach_calls,
        }),
    );

    let created = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Follow-up attach failure".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("first".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    let task_id = created.task.task_id.clone();
    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 10,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let error = service
        .prompt(SessionPromptParams {
            task_id: task_id.clone(),
            text: "follow-up".to_string(),
            prompt_attachments: Vec::new(),
            message_id: None,
        })
        .unwrap_err();
    assert!(matches!(error, RuntimeError::NotReady(_)));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(resumes.load(Ordering::SeqCst), 1);
    assert_eq!(prompts.load(Ordering::SeqCst), 1);
    assert_eq!(closes.load(Ordering::SeqCst), 0);

    let record = store.read_task(&task_id).unwrap();
    assert_eq!(record.status, TaskStatus::Inactive);
    assert!(record.active_turn_id.is_none());
    assert_eq!(record.agent_session_id.as_deref(), Some("session_followup_0"));

    let retry = service
        .prompt(SessionPromptParams {
            task_id: task_id.clone(),
            text: "retry".to_string(),
            prompt_attachments: Vec::new(),
            message_id: None,
        })
        .unwrap();
    assert!(matches!(
        retry.task.status,
        TaskStatus::Active | TaskStatus::Inactive
    ));
    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 10,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(resumes.load(Ordering::SeqCst), 2);
    assert_eq!(prompts.load(Ordering::SeqCst), 2);
}

#[test]
fn task_updates_emit_typed_task_updates() {
    let tmp = TempDir::new().unwrap();
    let (notifier, receiver) = TaskUpdateNotifier::channel();
    let runtime = Runtime::new_with_agent_and_task_update_notifier(
        tmp.path().join("store"),
        Arc::new(DelayedAgent),
        notifier,
        HostBridge::disabled(),
    )
    .unwrap();
    let created = runtime
        .service()
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Notify progress".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("Notify progress".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    let task_id = created.task.task_id;
    wait_until(|| {
        runtime
            .service()
            .snapshot(TaskSnapshotParams {
                task_id: task_id.clone(),
                tail_limit: 100,
            })
            .map(|snapshot| snapshot.task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let updates = (0..3)
        .filter_map(|_| receiver.recv_timeout(Duration::from_secs(1)).ok())
        .collect::<Vec<_>>();
    assert!(
        updates
            .iter()
            .any(|update| update.task_id == task_id),
        "expected task update for task"
    );
}
