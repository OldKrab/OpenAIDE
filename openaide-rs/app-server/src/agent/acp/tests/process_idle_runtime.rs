use super::*;
use crate::agent::TurnCancellation;

const SHORT_IDLE: Duration = Duration::from_millis(200);
const LONG_IDLE: Duration = Duration::from_millis(800);

fn fixture(temp: &tempfile::TempDir) -> AcpAgentRuntime {
    fixture_with_mode(temp, "normal")
}

fn fixture_with_mode(temp: &tempfile::TempDir, mode: &str) -> AcpAgentRuntime {
    fixture_with_host(temp, mode, HostBridge::disabled())
}

fn fixture_with_host(temp: &tempfile::TempDir, mode: &str, host: HostBridge) -> AcpAgentRuntime {
    let script = temp.path().join("agent.py");
    fs::write(&script, AGENT).unwrap();
    AcpAgentRuntime::new_with_host(
        AcpAgentConfig {
            agent_id: "codex".into(),
            command: "python3".into(),
            args: vec![script.to_string_lossy().into_owned()],
            env: vec![("IDLE_FIXTURE_MODE".into(), mode.into())],
            secret_env: Vec::new(),
        },
        host,
    )
    .with_process_idle_timeouts(SHORT_IDLE, LONG_IDLE)
}

fn list_pid(runtime: &AcpAgentRuntime, cwd: &Path) -> String {
    runtime
        .list_sessions(AgentListSessionsRequest {
            agent_id: "codex".into(),
            cwd: Some(cwd.to_string_lossy().into_owned()),
            cursor: None,
        })
        .unwrap()
        .sessions[0]
        .session_id
        .clone()
}

// `kill -0` checks liveness without sending a signal on both Linux and macOS.
fn process_exists(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .output()
        .is_ok_and(|result| result.status.success())
}

fn wait_for_exit(pid: &str) {
    wait_for_exit_within(pid, Duration::from_secs(3));
}

fn wait_for_exit_within(pid: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while process_exists(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_exists(pid),
        "idle Agent process {pid} stayed alive"
    );
}

#[test]
fn listing_only_process_expires_and_restarts_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture(&temp);
    let pid = list_pid(&runtime, temp.path());
    wait_for_exit_within(&pid, SHORT_IDLE * 3);
    let replacement_pid = list_pid(&runtime, temp.path());
    assert_ne!(replacement_pid, pid);
    wait_for_exit_within(&replacement_pid, SHORT_IDLE * 3);
}

#[test]
fn probing_without_opening_a_session_keeps_short_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture(&temp);
    runtime
        .probe(crate::agent::AgentProbeRequest {
            agent_id: "codex".into(),
        })
        .unwrap();
    let pid = list_pid(&runtime, temp.path());
    wait_for_exit_within(&pid, SHORT_IDLE * 3);
}

#[test]
fn forking_and_closing_a_session_preserves_long_retention_until_expiry() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture_with_mode(&temp, "fork");
    let forked = runtime
        .fork_session(crate::agent::AgentSessionFork {
            agent_id: "codex".into(),
            source_session_id: "existing-session".into(),
            cwd: temp.path().to_string_lossy().into_owned(),
            secret_resolver: None,
        })
        .unwrap();
    assert_eq!(forked.session_id, "forked-session");
    assert!(!forked.close_warning);
    let pid = list_pid(&runtime, temp.path());
    thread::sleep(SHORT_IDLE * 2);
    assert!(
        process_exists(&pid),
        "forking must promote even after closing the fork"
    );
    wait_for_exit(&pid);
}

#[test]
fn concurrent_discovery_after_expiry_shares_one_replacement_process() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture(&temp);
    let original = list_pid(&runtime, temp.path());
    wait_for_exit(&original);
    let barrier = std::sync::Barrier::new(8);
    let pids = thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    list_pid(&runtime, temp.path())
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    for pid in &pids {
        wait_for_exit(pid);
    }
    assert_ne!(pids[0], original);
    assert!(
        pids.iter().all(|pid| pid == &pids[0]),
        "concurrent callers launched different processes: {pids:?}"
    );
}

#[test]
fn creating_an_empty_session_promotes_retention_but_still_expires() {
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = crate::logging::capture_test_logs();
    let runtime = fixture(&temp);
    runtime
        .start_session(AgentSessionStart {
            agent_id: "codex".into(),
            task_id: "prepared".into(),
            cwd: temp.path().to_string_lossy().into_owned(),
            model_id: None,
            context: Vec::new(),
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .unwrap();
    let pid = list_pid(&runtime, temp.path());
    thread::sleep(SHORT_IDLE * 2);
    assert!(
        process_exists(&pid),
        "session creation must promote retention"
    );
    // The fixture deliberately does not advertise session/close. A Prepared
    // Task with no first prompt must nevertheless release its idle process.
    wait_for_exit(&pid);
    let events = diagnostics.snapshot();
    let terminal = events
        .iter()
        .find(|event| {
            event["event"] == "acp_agent_connection_completed"
                && event["fields"]["task_id"] == "prepared"
        })
        .expect("idle retirement must publish its terminal outcome");
    assert_eq!(terminal["level"], "info");
    assert_eq!(terminal["fields"]["outcome_kind"], "idle_timeout");
    assert!(
        events.iter().any(|event| {
            event["event"] == "acp_agent_connection_started"
                && event["fields"]["operation_id"] == terminal["fields"]["operation_id"]
        }),
        "the terminal event must correlate with the connection start"
    );
}

fn resume(runtime: &AcpAgentRuntime, cwd: &Path) -> AgentSession {
    runtime
        .resume_session(AgentSessionResume {
            agent_id: "codex".into(),
            task_id: "existing-task".into(),
            session_id: "existing-session".into(),
            cwd: cwd.to_string_lossy().into_owned(),
            model_id: None,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .unwrap()
}

#[test]
fn merely_resuming_a_session_keeps_short_retention_and_recovers_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture(&temp);
    let original = resume(&runtime, temp.path());
    let pid = list_pid(&runtime, temp.path());
    thread::sleep(SHORT_IDLE * 2);
    assert!(!process_exists(&pid), "resume must not promote retention");
    let restored = resume(&runtime, temp.path());
    assert_eq!(restored.session_id, original.session_id);
    let replacement_pid = list_pid(&runtime, temp.path());
    assert_ne!(replacement_pid, pid);
    wait_for_exit(&replacement_pid);
}

#[test]
fn loading_history_without_resume_support_keeps_short_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture_with_mode(&temp, "load_only");
    let loaded = runtime
        .load_session(crate::agent::AgentSessionLoad {
            agent_id: "codex".into(),
            task_id: "existing-task".into(),
            session_id: "existing-session".into(),
            cwd: temp.path().to_string_lossy().into_owned(),
            model_id: None,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .unwrap();
    assert_eq!(loaded.session.session_id, "existing-session");
    let pid = list_pid(&runtime, temp.path());
    wait_for_exit_within(&pid, SHORT_IDLE * 3);
}

#[test]
fn failed_prompt_releases_in_flight_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture_with_mode(&temp, "prompt_error");
    let session = resume(&runtime, temp.path());
    let pid = list_pid(&runtime, temp.path());
    let result = runtime.prompt(
        AgentPrompt {
            agent_id: "codex".into(),
            task_id: "existing-task".into(),
            session_id: session.session_id,
            text: "rejected".into(),
            attachments: Vec::new(),
            cancellation: TurnCancellation::new(),
        },
        Arc::new(CapturingEventSink::default()),
    );
    assert!(result.is_err());
    thread::sleep(SHORT_IDLE * 2);
    assert!(process_exists(&pid), "attempted work promotes retention");
    wait_for_exit(&pid);
}

#[test]
fn prompting_an_existing_session_promotes_and_suspends_expiration_until_response() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture(&temp);
    let session = resume(&runtime, temp.path());
    let pid = list_pid(&runtime, temp.path());
    let started = Instant::now();
    let outcome = runtime
        .prompt(
            AgentPrompt {
                agent_id: "codex".into(),
                task_id: "existing-task".into(),
                session_id: session.session_id,
                text: "slow".into(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(CapturingEventSink::default()),
        )
        .unwrap();
    assert_eq!(outcome, crate::agent::AgentPromptOutcome::EndTurn);
    assert!(
        started.elapsed() > LONG_IDLE,
        "fixture must outlast the idle deadline"
    );
    thread::sleep(SHORT_IDLE * 2);
    assert!(process_exists(&pid), "prompting must promote retention");
    wait_for_exit(&pid);
}

#[test]
fn agent_notifications_extend_short_retention_without_promoting_it() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture_with_mode(&temp, "notifications");
    let pid = list_pid(&runtime, temp.path());
    thread::sleep(SHORT_IDLE * 3);
    assert!(
        process_exists(&pid),
        "unsolicited notifications must renew retention"
    );
    // The fixture emits traffic for one second. With short retention it must
    // exit before a mistaken promotion's longer deadline would expire.
    wait_for_exit_within(&pid, Duration::from_millis(900));
}

#[test]
fn client_notifications_renew_short_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture(&temp);
    let session = resume(&runtime, temp.path());
    let pid = list_pid(&runtime, temp.path());
    for _ in 0..6 {
        thread::sleep(SHORT_IDLE / 2);
        runtime.cancel_session(&session.key()).unwrap();
        assert!(process_exists(&pid));
    }
    wait_for_exit_within(&pid, SHORT_IDLE * 3);
}

#[test]
fn repeated_discovery_renews_retention_without_promoting() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture(&temp);
    let pid = list_pid(&runtime, temp.path());
    for _ in 0..6 {
        thread::sleep(SHORT_IDLE / 2);
        assert_eq!(list_pid(&runtime, temp.path()), pid);
    }
    wait_for_exit_within(&pid, SHORT_IDLE * 3);
}

#[test]
fn slow_discovery_suspends_expiration_and_does_not_promote() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture_with_mode(&temp, "slow_list");
    let started = Instant::now();
    let pid = list_pid(&runtime, temp.path());
    assert!(started.elapsed() > LONG_IDLE);
    wait_for_exit_within(&pid, SHORT_IDLE * 3);
}

#[test]
fn stderr_output_does_not_keep_an_idle_process_alive() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture_with_mode(&temp, "stderr");
    let pid = list_pid(&runtime, temp.path());
    wait_for_exit_within(&pid, SHORT_IDLE * 3);
}

#[test]
fn replacement_process_does_not_inherit_long_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture(&temp);
    runtime
        .start_session(AgentSessionStart {
            agent_id: "codex".into(),
            task_id: "prepared-generation".into(),
            cwd: temp.path().to_string_lossy().into_owned(),
            model_id: None,
            context: Vec::new(),
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .unwrap();
    let original = list_pid(&runtime, temp.path());
    // Read-only traffic must renew the already-promoted process without
    // downgrading it. Each gap exceeds the short retention period.
    for _ in 0..3 {
        thread::sleep(SHORT_IDLE * 2);
        assert_eq!(list_pid(&runtime, temp.path()), original);
    }
    wait_for_exit(&original);
    let replacement = list_pid(&runtime, temp.path());
    assert_ne!(original, replacement);
    wait_for_exit_within(&replacement, SHORT_IDLE * 3);
}

#[test]
fn dropping_runtime_releases_process_before_long_idle_timeout() {
    let temp = tempfile::tempdir().unwrap();
    let runtime =
        fixture(&temp).with_process_idle_timeouts(Duration::from_secs(30), Duration::from_secs(60));
    let pid = list_pid(&runtime, temp.path());
    drop(runtime);
    wait_for_exit(&pid);
}

#[test]
fn pending_agent_request_suspends_expiration_until_client_response() {
    let temp = tempfile::tempdir().unwrap();
    let (host, requests) = HostBridge::channel();
    let runtime = fixture_with_host(&temp, "host_request", host.clone());
    let pid = list_pid(&runtime, temp.path());
    let request = requests.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(request.method, "fs/read_text_file");
    thread::sleep(SHORT_IDLE * 3);
    assert!(
        process_exists(&pid),
        "an unanswered Agent request is in flight"
    );
    assert!(host.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0", "id": request.id, "result": { "content": "fixture" },
    })));
    wait_for_exit_within(&pid, SHORT_IDLE * 3);
}

const AGENT: &str = r#"
import json, os, sys, time, threading

output_lock = threading.Lock()
def emit(message):
    with output_lock:
        print(json.dumps(message), flush=True)

def send_activity():
    for n in range(12):
        time.sleep(0.08)
        emit({'jsonrpc': '2.0', 'method': '_fixture/keepalive', 'params': {}})

def send_stderr():
    for n in range(40):
        print('fixture diagnostic', file=sys.stderr, flush=True)
        time.sleep(0.05)

if os.environ['IDLE_FIXTURE_MODE'] == 'notifications':
    threading.Thread(target=send_activity, daemon=True).start()
if os.environ['IDLE_FIXTURE_MODE'] == 'stderr':
    threading.Thread(target=send_stderr, daemon=True).start()

for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request or 'method' not in request:
        continue
    method = request['method']
    if method == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {'sessionCapabilities': {'list': {}, 'resume': {}}}, 'authMethods': []}
        if os.environ['IDLE_FIXTURE_MODE'] == 'load_only':
            result['agentCapabilities'] = {'loadSession': True, 'sessionCapabilities': {'list': {}}}
        if os.environ['IDLE_FIXTURE_MODE'] == 'fork':
            result['agentCapabilities']['loadSession'] = True
            result['agentCapabilities']['sessionCapabilities'].update({'fork': {}, 'close': {}})
    elif method == 'session/list':
        if os.environ['IDLE_FIXTURE_MODE'] == 'slow_list':
            time.sleep(1.0)
        result = {'sessions': [{'sessionId': str(os.getpid()), 'cwd': request['params']['cwd']}]}
    elif method == 'session/new':
        result = {'sessionId': 'empty-' + str(os.getpid())}
    elif method == 'session/fork':
        result = {'sessionId': 'forked-session'}
    elif method == 'session/close':
        result = {}
    elif method in ('session/resume', 'session/load'):
        result = {}
    elif method == 'session/prompt':
        if os.environ['IDLE_FIXTURE_MODE'] == 'prompt_error':
            emit({'jsonrpc': '2.0', 'id': request['id'], 'error': {'code': -32603, 'message': 'Fixture rejected prompt'}})
            continue
        time.sleep(1.0)
        result = {'stopReason': 'end_turn'}
    else:
        emit({'jsonrpc': '2.0', 'id': request['id'], 'error': {'code': -32601, 'message': 'Method not found'}})
        continue
    emit({'jsonrpc': '2.0', 'id': request['id'], 'result': result})
    if method == 'session/list' and os.environ['IDLE_FIXTURE_MODE'] == 'host_request':
        # Both peers may use the same request ID; lifetime must keep their
        # pending requests in independent namespaces.
        emit({'jsonrpc': '2.0', 'id': request['id'], 'method': 'fs/read_text_file', 'params': {'sessionId': 'existing-session', 'path': request['params']['cwd'] + '/fixture.txt'}})
"#;
