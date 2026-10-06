use super::*;
use crate::agent::status_cache::{AgentStatusCache, AgentStatusUpdateReceiver};
use crate::agent::status_recording_runtime::AgentStatusRecordingRuntime;
use crate::agent::AgentProbeRequest;
use crate::protocol::model::{
    AgentAuthMethodSummary, AgentProbeCapabilities, AgentProbeResult, AgentProbeStatus,
};
use openaide_app_server_protocol::snapshot::TaskNavigationRefreshState;

#[test]
fn sidebar_refresh_runs_independent_contexts_before_waiting_for_slow_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    for index in 0..3 {
        store
            .write_task(&task_record(
                &format!("task-{index}"),
                temp.path()
                    .join(format!("workspace-{index}"))
                    .to_str()
                    .unwrap(),
            ))
            .unwrap();
    }
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let agent = Arc::new(MixedCatalogRuntime {
        authenticated: AtomicBool::new(true),
        listing_gate: Some(gate.clone()),
        listing_entered: Some(entered_tx),
        ..Default::default()
    });
    let (notifier, updates) = TaskUpdateNotifier::channel();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent,
        notifier,
    )
    .unwrap();
    api.request_native_session_catalog_refresh();
    let expected = 3 * BUILT_IN_AGENT_METADATA.len();
    let mut entered = 0;
    for _ in 0..expected {
        if entered_rx.recv_timeout(crate::test_sync::WATCHDOG).is_err() {
            break;
        }
        entered += 1;
    }
    // Release and drain before asserting, including the intentionally failing run.
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    loop {
        let update = updates.recv_timeout(crate::test_sync::WATCHDOG).unwrap();
        if matches!(
            update.kind,
            TaskUpdateKind::NavigationRefreshStateChanged {
                refresh: TaskNavigationRefreshState::Idle
            }
        ) {
            break;
        }
    }
    assert_eq!(entered, expected,
        "one slow history must not serialize every Agent/workspace and keep sidebar Refresh disabled");
    assert!(!api.native_session_catalog().refreshing());
}

#[test]
fn unsigned_agent_catalog_refresh_settles_and_retries_after_explicit_refresh_or_sign_in() {
    assert_unsigned_catalog_settles(true);
}

#[test]
fn methodless_unsigned_agent_catalog_refresh_does_not_oscillate_through_connected() {
    assert_unsigned_catalog_settles(false);
}

fn assert_unsigned_catalog_settles(advertises_auth_method: bool) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("state")).unwrap();
    for index in 0..3 {
        let workspace = temp.path().join(format!("workspace-{index}"));
        store
            .write_task(&task_record(
                &format!("task-{index}"),
                workspace.to_str().unwrap(),
            ))
            .unwrap();
    }
    let (statuses, updates) = AgentStatusCache::channel();
    let agent = Arc::new(MixedCatalogRuntime {
        advertises_auth_method,
        ..Default::default()
    });
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        AgentStatusRecordingRuntime::wrap(agent.clone(), statuses.clone()),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();

    api.request_native_session_catalog_refresh();
    assert!(
        publish_status_updates_until_settled(&api, &updates),
        "unchanged auth failures must not feed another catalog refresh forever"
    );
    assert!(matches!(
        api.native_session_catalog().refresh_state(),
        TaskNavigationRefreshState::Failed { .. }
    ));
    let contexts = 3;
    let agent_count = BUILT_IN_AGENT_METADATA.len();
    assert_eq!(
        api.native_session_catalog().entries().len(),
        contexts * (agent_count - 1)
    );
    let before_retry = agent.calls.lock().unwrap().clone();
    assert_eq!(
        before_retry.len(),
        contexts * agent_count,
        "every Agent covers every context"
    );

    api.request_native_session_catalog_refresh();
    assert!(publish_status_updates_until_settled(&api, &updates));
    let after_retry = agent.calls.lock().unwrap().clone();
    for (context, count) in &before_retry {
        assert_eq!(
            after_retry[context],
            count + 1,
            "explicit refresh still retries"
        );
    }

    agent.authenticated.store(true, Ordering::SeqCst);
    statuses.record_authentication_success("codex");
    assert!(publish_status_updates_until_settled(&api, &updates));
    assert_eq!(
        api.native_session_catalog().refresh_state(),
        TaskNavigationRefreshState::Idle
    );
    assert_eq!(
        api.native_session_catalog().entries().len(),
        contexts * agent_count
    );
}

/// Reproduces the process owner's status receiver -> catalog refresh feedback.
/// The bounded failure path stops forwarding and drains the worker before its
/// temporary store disappears, including when the regression is intentionally red.
fn publish_status_updates_until_settled(
    api: &TaskProductApi,
    updates: &AgentStatusUpdateReceiver,
) -> bool {
    // The bound counts the refreshes that status updates feed, not elapsed time:
    // a slow refresh keeps the loop waiting and never reads as an endless one.
    let mut fed_refreshes = 0;
    while fed_refreshes < 32 {
        // timing: absence — a quiet window after the refresh ended means it settled.
        match updates.recv_timeout(crate::test_sync::ABSENCE_WINDOW) {
            Ok(()) => {
                fed_refreshes += 1;
                api.request_native_session_catalog_refresh();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // A refresh that never ends is caught by the watchdog here.
                wait_until(|| !api.native_session_catalog().refreshing());
                if updates.try_recv().is_err() {
                    return true;
                }
                fed_refreshes += 1;
                api.request_native_session_catalog_refresh();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    wait_until(|| !api.native_session_catalog().refreshing());
    false
}

#[derive(Default)]
struct MixedCatalogRuntime {
    authenticated: AtomicBool,
    calls: Mutex<HashMap<(String, String), usize>>,
    advertises_auth_method: bool,
    listing_gate: Option<Arc<(Mutex<bool>, std::sync::Condvar)>>,
    listing_entered: Option<std::sync::mpsc::Sender<()>>,
}

impl AgentRuntime for MixedCatalogRuntime {
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Ok(AgentProbeResult {
            agent_id: request.agent_id.clone(),
            status: AgentProbeStatus::Ready,
            protocol_version: "fixture".to_string(),
            implementation_name: None,
            implementation_version: None,
            capabilities: Vec::new(),
            typed_capabilities: AgentProbeCapabilities::default(),
            auth_methods: if request.agent_id == "codex" && self.advertises_auth_method {
                vec![AgentAuthMethodSummary {
                    id: "fixture-sign-in".to_string(),
                    label: "Fixture sign-in".to_string(),
                    kind: "agent".to_string(),
                    description: None,
                    variables: Vec::new(),
                    link: None,
                    terminal_args: Vec::new(),
                    terminal_env: Default::default(),
                }]
            } else {
                Vec::new()
            },
            logout_supported: false,
        })
    }

    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        if let Some(entered) = &self.listing_entered {
            entered.send(()).unwrap();
        }
        if let Some(gate) = &self.listing_gate {
            drop(
                gate.1
                    .wait_while(gate.0.lock().unwrap(), |released| !*released)
                    .unwrap(),
            );
        }
        let cwd = request.cwd.unwrap();
        *self
            .calls
            .lock()
            .unwrap()
            .entry((request.agent_id.clone(), cwd.clone()))
            .or_default() += 1;
        if request.agent_id == "codex" && !self.authenticated.load(Ordering::SeqCst) {
            return Err(RuntimeError::AuthRequired(
                "Fixture sign-in required".to_string(),
            ));
        }
        Ok(AgentListSessionsResult {
            authoritative: true,
            agent_id: request.agent_id.clone(),
            sessions: vec![AgentListedSession {
                session_id: format!(
                    "{}-{}",
                    request.agent_id,
                    project_id_for_workspace(&cwd).as_str()
                ),
                cwd,
                title: Some("Fixture history".to_string()),
                last_activity: None,
                updated_at: None,
            }],
            next_cursor: None,
        })
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        unreachable!("catalog discovery must not start sessions")
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        unreachable!("catalog discovery must not prompt")
    }
}
