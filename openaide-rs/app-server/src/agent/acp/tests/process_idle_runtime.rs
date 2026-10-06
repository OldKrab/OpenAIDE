use super::*;
use crate::agent::{AgentPromptOutcome, TurnCancellation};
use crate::test_sync::{wait_for, wait_until, EXPIRES, NEVER};

// Pairing `EXPIRES` with `NEVER` turns "which retention applies" into "does the
// process exit", which is awaited instead of timed. Renewal and suspension
// arithmetic is proven on a paused clock in `acp_process_lifetime_tests.rs`.
/// The process exits only while it is still on short retention.
fn expires_unless_promoted(temp: &tempfile::TempDir, mode: &str) -> AcpAgentRuntime {
    fixture_with_host(temp, mode, HostBridge::disabled()).with_process_idle_timeouts(EXPIRES, NEVER)
}

// Fixture traffic renews this retention, so it sits two orders of magnitude
// above the traffic cadence instead of racing it.
// timing: contract — renewal by traffic is an elapsed-time behavior.
const RENEWED: Duration = Duration::from_secs(1);

/// Short retention that fixture traffic keeps renewing until the fixture goes quiet.
fn renewed_unless_promoted(temp: &tempfile::TempDir, mode: &str) -> AcpAgentRuntime {
    fixture_with_host(temp, mode, HostBridge::disabled()).with_process_idle_timeouts(RENEWED, NEVER)
}

/// The process exits only after session work promoted it to long retention.
fn expires_once_promoted(temp: &tempfile::TempDir, mode: &str) -> AcpAgentRuntime {
    fixture_with_host(temp, mode, HostBridge::disabled()).with_process_idle_timeouts(NEVER, EXPIRES)
}

fn fixture_with_host(temp: &tempfile::TempDir, mode: &str, host: HostBridge) -> AcpAgentRuntime {
    let script = temp.path().join("agent.py");
    fs::write(&script, AGENT).unwrap();
    AcpAgentRuntime::new_with_host(
        AcpAgentConfig {
            agent_id: "codex".into(),
            command: "python3".into(),
            args: vec![script.to_string_lossy().into_owned()],
            env: vec![
                ("IDLE_FIXTURE_MODE".into(), mode.into()),
                (
                    "IDLE_FIXTURE_GATE".into(),
                    temp.path().join("gate").to_string_lossy().into_owned(),
                ),
            ],
            secret_env: Vec::new(),
        },
        host,
    )
}

/// Waits until the fixture reports that it is holding at its gate, and returns its pid.
fn wait_until_held(temp: &tempfile::TempDir) -> String {
    let held = temp.path().join("gate.held");
    // The fixture renames a complete file into place, so a read never sees part of it.
    wait_for("the fixture to reach its gate", || {
        fs::read_to_string(&held).ok()
    })
}

fn release(temp: &tempfile::TempDir) {
    fs::write(temp.path().join("gate.release"), "release").unwrap();
}

/// Absence window: lets an idle deadline pass while the fixture holds the process in
/// the state under test. The hold ends only on `release`, so a slow runner lengthens
/// the window and cannot fail the assertion that follows.
fn outlast_retention() {
    crate::test_sync::outlast(EXPIRES);
}

fn outlast_renewed_retention() {
    crate::test_sync::outlast(RENEWED);
}

fn list(runtime: &AcpAgentRuntime, cwd: &Path) -> Result<AgentListSessionsResult, RuntimeError> {
    runtime.list_sessions(AgentListSessionsRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        agent_id: "codex".into(),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        cursor: None,
    })
}

fn list_pid(runtime: &AcpAgentRuntime, cwd: &Path) -> String {
    list(runtime, cwd).unwrap().sessions[0].session_id.clone()
}

// `kill -0` checks liveness without sending a signal on both Linux and macOS.
fn process_exists(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .output()
        .is_ok_and(|result| result.status.success())
}

fn wait_for_exit(pid: &str) {
    wait_until("the Agent process to exit", || !process_exists(pid));
}

/// Starts a session and returns the pid of the process that created it.
fn start(runtime: &AcpAgentRuntime, cwd: &Path, task_id: &str) -> String {
    let session = runtime
        .start_session(AgentSessionStart {
            agent_id: "codex".into(),
            task_id: task_id.into(),
            cwd: cwd.to_string_lossy().into_owned(),
            model_id: None,
            context: Vec::new(),
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .unwrap();
    session
        .session_id
        .strip_prefix("empty-")
        .expect("fixture session id carries its pid")
        .to_string()
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

fn prompt(
    runtime: &AcpAgentRuntime,
    session: AgentSession,
) -> Result<AgentPromptOutcome, RuntimeError> {
    runtime.prompt(
        AgentPrompt {
            agent_id: "codex".into(),
            task_id: "existing-task".into(),
            session_id: session.session_id,
            text: "work".into(),
            attachments: Vec::new(),
            cancellation: TurnCancellation::new(),
        },
        Arc::new(CapturingEventSink::default()),
    )
}

#[test]
fn listing_only_process_expires_and_restarts_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_unless_promoted(&temp, "normal");
    let pid = list_pid(&runtime, temp.path());
    wait_for_exit(&pid);
    let replacement_pid = list_pid(&runtime, temp.path());
    assert_ne!(replacement_pid, pid);
    wait_for_exit(&replacement_pid);
}

#[test]
fn unanswered_listing_expires_after_its_request_deadline() {
    let temp = tempfile::tempdir().unwrap();
    // The Agent never answers, so the request deadline always fires.
    let runtime = expires_unless_promoted(&temp, "unanswered_list");
    let pid = list_pid(&runtime, temp.path());
    // Only the unanswered request runs under the deadline that expires.
    let runtime = runtime.with_list_timeout(EXPIRES);
    let result = runtime.list_sessions(AgentListSessionsRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        agent_id: "codex".into(),
        cwd: Some(temp.path().to_string_lossy().into_owned()),
        cursor: Some("unanswered".into()),
    });
    assert!(
        matches!(result, Err(crate::protocol::errors::RuntimeError::NotReady(message))
        if message.contains("timed out"))
    );
    wait_for_exit(&pid);
    let runtime = runtime.with_list_timeout(crate::test_sync::WATCHDOG);
    assert_ne!(list_pid(&runtime, temp.path()), pid);
}

#[test]
fn probing_without_opening_a_session_keeps_short_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_unless_promoted(&temp, "normal");
    runtime
        .probe(crate::agent::AgentProbeRequest {
            agent_id: "codex".into(),
        })
        .unwrap();
    let pid = list_pid(&runtime, temp.path());
    wait_for_exit(&pid);
}

#[test]
fn forking_and_closing_a_session_preserves_long_retention_until_expiry() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_once_promoted(&temp, "fork");
    let pid = list_pid(&runtime, temp.path());
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
    // Forking promotes even though the fork is closed again.
    wait_for_exit(&pid);
}

#[test]
fn concurrent_discovery_after_expiry_shares_one_replacement_process() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_unless_promoted(&temp, "normal");
    let original = list_pid(&runtime, temp.path());
    wait_for_exit(&original);
    // The replacement stays retained until every caller has listed, so a caller
    // scheduled late still finds the process its peers launched.
    runtime.set_process_idle_timeouts(NEVER, NEVER);
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
    runtime.set_process_idle_timeouts(EXPIRES, NEVER);
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
    let runtime = expires_once_promoted(&temp, "normal");
    let pid = start(&runtime, temp.path(), "prepared");
    // The fixture deliberately does not advertise session/close. A Prepared
    // Task with no first prompt must nevertheless release its idle process.
    wait_for_exit(&pid);
    // The terminal outcome is published once the connection task observes the exit.
    // Each snapshot drains the capture, so events accumulate across polls.
    let mut events = Vec::new();
    let terminal = wait_for("idle retirement to publish its terminal outcome", || {
        events.extend(diagnostics.snapshot());
        events
            .iter()
            .find(|event| {
                event["event"] == "acp_agent_connection_completed"
                    && event["fields"]["task_id"] == "prepared"
            })
            .cloned()
    });
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

#[test]
fn idle_session_close_retains_process_until_its_response() {
    let temp = tempfile::tempdir().unwrap();
    // A listing the fixture holds retains the process until the idle close
    // arrives. The fixture then answers the listing and holds the close, so
    // from there only the close operation retains the process.
    // timing: expiry — the session idles out at once; the fixture holds its close.
    let idle = Duration::from_millis(1);
    let runtime = expires_unless_promoted(&temp, "held_close")
        .with_list_timeout(NEVER)
        .with_session_idle_timeout(idle)
        .with_session_idle_close_timeout(NEVER);
    thread::scope(|scope| {
        let listing = scope.spawn(|| list(&runtime, temp.path()));
        wait_until("the fixture to hold the listing", || {
            temp.path().join("gate.listing").exists()
        });
        resume(&runtime, temp.path());
        let pid = wait_until_held(&temp);
        listing.join().unwrap().unwrap();
        outlast_retention();
        assert!(
            process_exists(&pid),
            "idle close must retain its Agent process"
        );
        release(&temp);
        wait_for_exit(&pid);
    });
}

#[test]
fn merely_resuming_a_session_keeps_short_retention_and_recovers_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_unless_promoted(&temp, "normal");
    let original = resume(&runtime, temp.path());
    let pid = list_pid(&runtime, temp.path());
    wait_for_exit(&pid);
    let restored = resume(&runtime, temp.path());
    assert_eq!(restored.session_id, original.session_id);
    let replacement_pid = list_pid(&runtime, temp.path());
    assert_ne!(replacement_pid, pid);
    wait_for_exit(&replacement_pid);
}

#[test]
fn loading_history_without_resume_support_keeps_short_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_unless_promoted(&temp, "load_only");
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
    wait_for_exit(&pid);
}

#[test]
fn failed_prompt_promotes_and_releases_in_flight_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_once_promoted(&temp, "prompt_error");
    let session = resume(&runtime, temp.path());
    let pid = list_pid(&runtime, temp.path());
    assert!(prompt(&runtime, session).is_err());
    // Attempted work promotes retention, and the failed request no longer holds it.
    wait_for_exit(&pid);
}

#[test]
fn prompting_an_existing_session_promotes_and_suspends_expiration_until_response() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_once_promoted(&temp, "held_prompt");
    let session = resume(&runtime, temp.path());
    thread::scope(|scope| {
        let prompting = scope.spawn(|| prompt(&runtime, session));
        let pid = wait_until_held(&temp);
        outlast_retention();
        assert!(process_exists(&pid), "an unanswered prompt is in flight");
        release(&temp);
        assert_eq!(
            prompting.join().unwrap().unwrap(),
            AgentPromptOutcome::EndTurn
        );
        wait_for_exit(&pid);
    });
}

#[test]
fn agent_notifications_extend_short_retention_without_promoting_it() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = renewed_unless_promoted(&temp, "notifications");
    let pid = list_pid(&runtime, temp.path());
    outlast_renewed_retention();
    assert!(
        process_exists(&pid),
        "unsolicited notifications must renew retention"
    );
    // The fixture stops its traffic; the process exits only if it was not promoted.
    release(&temp);
    wait_for_exit(&pid);
}

#[test]
fn slow_discovery_suspends_expiration_and_does_not_promote() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_unless_promoted(&temp, "held_list");
    thread::scope(|scope| {
        let listing = scope.spawn(|| list_pid(&runtime, temp.path()));
        let pid = wait_until_held(&temp);
        outlast_retention();
        assert!(process_exists(&pid), "an unanswered listing is in flight");
        release(&temp);
        assert_eq!(listing.join().unwrap(), pid);
        wait_for_exit(&pid);
    });
}

#[test]
fn stderr_output_does_not_keep_an_idle_process_alive() {
    let temp = tempfile::tempdir().unwrap();
    // The fixture writes to stderr for as long as it lives.
    let runtime = expires_unless_promoted(&temp, "stderr");
    let pid = list_pid(&runtime, temp.path());
    wait_for_exit(&pid);
}

#[test]
fn replacement_process_does_not_inherit_long_retention() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = expires_unless_promoted(&temp, "exit_on_release");
    let original = start(&runtime, temp.path(), "prepared-generation");
    // The promoted process cannot expire here, so it ends on its own.
    release(&temp);
    wait_for_exit(&original);
    // A listing can still reach the ended process until its teardown is observed.
    let replacement = wait_for("a listing served by a replacement process", || {
        list(&runtime, temp.path())
            .ok()
            .map(|listed| listed.sessions[0].session_id.clone())
    });
    assert_ne!(original, replacement);
    wait_for_exit(&replacement);
}

#[test]
fn dropping_runtime_releases_process_before_its_idle_timeout() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = fixture_with_host(&temp, "normal", HostBridge::disabled())
        .with_process_idle_timeouts(NEVER, NEVER);
    let pid = list_pid(&runtime, temp.path());
    drop(runtime);
    wait_for_exit(&pid);
}

#[test]
fn pending_agent_request_suspends_expiration_until_client_response() {
    let temp = tempfile::tempdir().unwrap();
    let (host, requests) = HostBridge::channel();
    let runtime = fixture_with_host(&temp, "host_request", host.clone())
        .with_process_idle_timeouts(NEVER, NEVER);
    let pid = list_pid(&runtime, temp.path());
    let request = requests.recv_timeout(crate::test_sync::WATCHDOG).unwrap();
    assert_eq!(request.method, "fs/read_text_file");
    // The fixture sends its request after the listing response, so retention
    // starts expiring only once the request is pending.
    runtime.set_process_idle_timeouts(EXPIRES, NEVER);
    outlast_retention();
    assert!(
        process_exists(&pid),
        "an unanswered Agent request is in flight"
    );
    assert!(host.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0", "id": request.id, "result": { "content": "fixture" },
    })));
    wait_for_exit(&pid);
}

#[test]
fn probe_timeout_stops_a_process_that_never_initializes() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = std::sync::Arc::new(
        fixture_with_host(&temp, "held_initialize", HostBridge::disabled())
            .with_process_idle_timeouts(NEVER, NEVER),
    );
    // Listing launches the process, which then holds inside `initialize`.
    // TODO: a listing in flight when its process is stopped waits out its whole
    // request deadline instead of failing with the process. Fail it at the stop,
    // then join this thread and assert its error.
    let cwd = temp.path().to_path_buf();
    let listing_runtime = runtime.clone();
    thread::spawn(move || list(&listing_runtime, &cwd));
    let pid = wait_until_held(&temp);
    let error = runtime
        .probe_with_timeout(
            AgentProbeRequest {
                agent_id: "codex".into(),
            },
            EXPIRES,
        )
        .unwrap_err();
    assert!(matches!(error, RuntimeError::NotReady(_)), "{error}");
    assert!(
        error.to_string().contains("ACP Agent probe timed out"),
        "{error}"
    );
    wait_for_exit(&pid);
}

const AGENT: &str = r#"
import json, os, sys, time, threading

mode = os.environ['IDLE_FIXTURE_MODE']
gate = os.environ['IDLE_FIXTURE_GATE']
output_lock = threading.Lock()
held_listing = None

def emit(message):
    with output_lock:
        print(json.dumps(message), flush=True)

def released():
    return os.path.exists(gate + '.release')

# Reports that the fixture reached its gate, then waits for the test to release it.
# Tests order the fixture with this gate instead of a delay.
def hold():
    with open(gate + '.tmp', 'w') as held:
        held.write(str(os.getpid()))
    os.rename(gate + '.tmp', gate + '.held')
    while not released():
        time.sleep(0.01)  # timing: poll

def send_activity():
    while not released():
        time.sleep(0.01)  # timing: contract — traffic cadence inside the retention it renews
        emit({'jsonrpc': '2.0', 'method': '_fixture/keepalive', 'params': {}})

def send_stderr():
    while True:
        print('fixture diagnostic', file=sys.stderr, flush=True)
        time.sleep(0.01)  # timing: data — stderr pacing; no outcome waits on it

def exit_on_release():
    # Claiming the release keeps the replacement process alive.
    while True:
        try:
            os.rename(gate + '.release', gate + '.claimed')
            # A clean exit must end the connection just as a crash does.
            os._exit(0)
        except FileNotFoundError:
            time.sleep(0.01)  # timing: poll

if mode == 'notifications':
    threading.Thread(target=send_activity, daemon=True).start()
if mode == 'stderr':
    threading.Thread(target=send_stderr, daemon=True).start()
if mode == 'exit_on_release':
    threading.Thread(target=exit_on_release, daemon=True).start()

for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request or 'method' not in request:
        continue
    method = request['method']
    if method == 'initialize':
        if mode == 'held_initialize':
            hold()
        result = {'protocolVersion': 1, 'agentCapabilities': {'sessionCapabilities': {'list': {}, 'resume': {}}}, 'authMethods': []}
        if mode == 'load_only':
            result['agentCapabilities'] = {'loadSession': True, 'sessionCapabilities': {'list': {}}}
        if mode == 'fork':
            result['agentCapabilities']['loadSession'] = True
            result['agentCapabilities']['sessionCapabilities'].update({'fork': {}, 'close': {}})
        if mode == 'held_close':
            result['agentCapabilities']['sessionCapabilities']['close'] = {}
    elif method == 'session/list':
        if mode == 'unanswered_list' and request['params'].get('cursor') == 'unanswered':
            continue
        if mode == 'held_list':
            hold()
        if mode == 'held_close':
            # This request retains the process until the idle close arrives.
            held_listing = request
            open(gate + '.listing', 'w').close()
            continue
        result = {'sessions': [{'sessionId': str(os.getpid()), 'cwd': request['params']['cwd']}]}
    elif method == 'session/new':
        result = {'sessionId': 'empty-' + str(os.getpid())}
    elif method == 'session/fork':
        result = {'sessionId': 'forked-session'}
    elif method == 'session/close':
        if mode == 'held_close':
            # Only the pending close may retain the process from here on.
            emit({'jsonrpc': '2.0', 'id': held_listing['id'], 'result': {'sessions': []}})
            hold()
        result = {}
    elif method in ('session/resume', 'session/load'):
        result = {}
    elif method == 'session/prompt':
        if mode == 'prompt_error':
            emit({'jsonrpc': '2.0', 'id': request['id'], 'error': {'code': -32603, 'message': 'Fixture rejected prompt'}})
            continue
        if mode == 'held_prompt':
            hold()
        result = {'stopReason': 'end_turn'}
    else:
        emit({'jsonrpc': '2.0', 'id': request['id'], 'error': {'code': -32601, 'message': 'Method not found'}})
        continue
    emit({'jsonrpc': '2.0', 'id': request['id'], 'result': result})
    if method == 'session/list' and mode == 'host_request':
        # Both peers may use the same request ID; lifetime must keep their
        # pending requests in independent namespaces.
        emit({'jsonrpc': '2.0', 'id': request['id'], 'method': 'fs/read_text_file', 'params': {'sessionId': 'existing-session', 'path': request['params']['cwd'] + '/fixture.txt'}})
"#;
