use super::*;
use crate::agent::status_cache::{AgentStatusCache, AgentStatusUpdateReceiver};
use crate::agent::status_recording_runtime::AgentStatusRecordingRuntime;
use crate::agent::AgentProbeRequest;
use crate::protocol::model::{
    AgentAuthMethodSummary, AgentProbeCapabilities, AgentProbeResult, AgentProbeStatus,
};
use openaide_app_server_protocol::snapshot::TaskNavigationRefreshState;

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
    assert_eq!(api.native_session_catalog().entries().len(), 3);
    let before_retry = agent.calls.lock().unwrap().clone();
    assert_eq!(before_retry.len(), 6, "two Agents cover three contexts");

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
    assert_eq!(api.native_session_catalog().entries().len(), 6);
}

/// Reproduces the process owner's status receiver -> catalog refresh feedback.
/// The bounded failure path stops forwarding and drains the worker before its
/// temporary store disappears, including when the regression is intentionally red.
fn publish_status_updates_until_settled(
    api: &TaskProductApi,
    updates: &AgentStatusUpdateReceiver,
) -> bool {
    for _ in 0..32 {
        match updates.recv_timeout(Duration::from_millis(100)) {
            Ok(()) => api.request_native_session_catalog_refresh(),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                if !api.native_session_catalog().refreshing() =>
            {
                return true;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
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
