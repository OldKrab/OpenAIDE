#[test]
fn passive_snapshot_does_not_call_agent() {
    let tmp = TempDir::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let state = Arc::new((
        Mutex::new(PassiveSnapshotPromptState::default()),
        Condvar::new(),
    ));
    let agent = Arc::new(PassiveSnapshotAgent {
        calls: calls.clone(),
        state: state.clone(),
    });
    let store = Store::open(tmp.path().join("store")).unwrap();
    let service = TaskService::new(store, agent);

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Passive check".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("check passive open".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    wait_until(|| {
        let (state_lock, _) = &*state;
        state_lock.lock().unwrap().parked
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let _passive = service
        .snapshot(TaskSnapshotParams {
            task_id: snapshot.task.task_id,
            tail_limit: 10,
        })
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    {
        let (state_lock, changed) = &*state;
        let mut state = state_lock.lock().unwrap();
        state.released = true;
        changed.notify_all();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn cancel_stops_pending_agent_turn() {
    let tmp = TempDir::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let agent = Arc::new(CountingAgent {
        calls: calls.clone(),
    });
    let store = Store::open(tmp.path().join("store")).unwrap();
    let service = TaskService::new(store, agent);

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Cancel check".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("start then stop".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    assert_eq!(snapshot.task.status, TaskStatus::Active);

    let stopped = service
        .cancel(openaide_app_server::protocol::params::TaskIdParams {
            task_id: snapshot.task.task_id.clone(),
        })
        .unwrap();
    // The Agent may acknowledge cancellation before cancel() reads its return snapshot.
    assert!(matches!(
        stopped.task.status,
        TaskStatus::Stopping | TaskStatus::Inactive
    ));
    assert_eq!(
        has_running_activity(&stopped),
        stopped.task.status == TaskStatus::Stopping
    );
    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: snapshot.task.task_id.clone(),
                tail_limit: 10,
            })
            .map(|snapshot| {
                snapshot
                    .chat
                    .items
                    .iter()
                    .any(|item| item.message_type == "interruption")
            })
            .unwrap_or(false)
    });

    thread::sleep(Duration::from_millis(140));
    let passive = service
        .snapshot(TaskSnapshotParams {
            task_id: snapshot.task.task_id,
            tail_limit: 10,
        })
        .unwrap();
    assert_eq!(passive.task.status, TaskStatus::Inactive);
    assert!(!has_running_activity(&passive));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!passive
        .chat
        .items
        .iter()
        .any(|item| item.message_type == "agent_message"));
}

#[test]
fn prompt_rejects_double_turn_while_active() {
    let tmp = TempDir::new().unwrap();
    let started = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicUsize::new(0));
    let agent = Arc::new(WaitingAgent {
        started: started.clone(),
        cancelled,
    });
    let store = Store::open(tmp.path().join("store")).unwrap();
    let service = TaskService::new(store, agent);

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Double turn".to_string(),
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
    let task_id = snapshot.task.task_id;
    wait_until(|| started.load(Ordering::SeqCst) == 1);

    let error = service
        .prompt(SessionPromptParams {
            task_id: task_id.clone(),
            text: "second".to_string(),
            prompt_attachments: Vec::new(),
            message_id: None,
        })
        .unwrap_err();

    assert!(matches!(error, RuntimeError::InvalidParams(_)));
    service.cancel(TaskIdParams { task_id }).unwrap();
}

#[test]
fn cancel_signals_agent_after_turn_started() {
    let tmp = TempDir::new().unwrap();
    let started = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicUsize::new(0));
    let agent = Arc::new(WaitingAgent {
        started: started.clone(),
        cancelled: cancelled.clone(),
    });
    let store = Store::open(tmp.path().join("store")).unwrap();
    let service = TaskService::new(store, agent);

    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Started cancel".to_string(),
            workspace_root: tmp.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("wait for cancel".to_string()),
            external_session_id: None,
            model_id: None,
            config_options: None,
            context: Vec::new(),
        })
        .unwrap();
    wait_until(|| started.load(Ordering::SeqCst) == 1);

    let stopped = service
        .cancel(openaide_app_server::protocol::params::TaskIdParams {
            task_id: snapshot.task.task_id.clone(),
        })
        .unwrap();
    assert!(matches!(
        stopped.task.status,
        TaskStatus::Stopping | TaskStatus::Inactive
    ));

    wait_until(|| cancelled.load(Ordering::SeqCst) == 1);
    wait_until(|| {
        service
            .snapshot(TaskSnapshotParams {
                task_id: snapshot.task.task_id.clone(),
                tail_limit: 20,
            })
            .is_ok_and(|snapshot| snapshot.task.status == TaskStatus::Inactive)
    });
    let passive = service
        .snapshot(TaskSnapshotParams {
            task_id: snapshot.task.task_id,
            tail_limit: 20,
        })
        .unwrap();
    assert!(passive
        .chat
        .items
        .iter()
        .any(|item| item.message_type == "agent_message"));
    assert!(!has_running_activity(&passive));
}
