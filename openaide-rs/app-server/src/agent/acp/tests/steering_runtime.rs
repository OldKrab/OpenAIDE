use super::*;
use crate::agent::{AgentPromptOutcome, AgentSessionSetConfigOptionRequest, TurnCancellation};
use std::sync::{mpsc, Arc};

struct Fixture {
    runtime: Arc<AcpAgentRuntime>,
    log: PathBuf,
    sink: Arc<CapturingEventSink>,
    _temp: tempfile::TempDir,
}

impl Fixture {
    fn new(mode: &str) -> Option<Self> {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping ACP steering fixture: python3 not found");
            return None;
        }
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("agent.py");
        let log = temp.path().join("calls.jsonl");
        std::fs::write(&script, AGENT).unwrap();
        let runtime = Arc::new(AcpAgentRuntime::new(AcpAgentConfig {
            agent_id: "codex".into(),
            command: "python3".into(),
            args: vec![
                script.to_string_lossy().into(),
                log.to_string_lossy().into(),
                mode.into(),
            ],
            env: Vec::new(),
            secret_env: Vec::new(),
        }));
        runtime
            .start_session(AgentSessionStart {
                agent_id: "codex".into(),
                task_id: "task".into(),
                cwd: temp.path().to_string_lossy().into(),
                model_id: None,
                context: Vec::new(),
                cancellation: TurnCancellation::new(),
                secret_resolver: None,
            })
            .unwrap();
        Some(Self {
            runtime,
            log,
            sink: Arc::default(),
            _temp: temp,
        })
    }

    fn prompt(&self, text: &str) -> AgentPrompt {
        AgentPrompt {
            agent_id: "codex".into(),
            task_id: "task".into(),
            session_id: "session".into(),
            text: text.into(),
            attachments: Vec::new(),
            cancellation: TurnCancellation::new(),
        }
    }

    fn start(&self) -> mpsc::Receiver<Result<AgentPromptOutcome, RuntimeError>> {
        let runtime = self.runtime.clone();
        let prompt = self.prompt("start");
        let sink = self.sink.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(runtime.prompt(prompt, sink));
        });
        self.wait_for("session/prompt", 1);
        rx
    }

    fn calls(&self, method: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|value| value["method"] == method)
            .collect()
    }

    fn wait_for(&self, method: &str, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.calls(method).len() < count {
            assert!(
                Instant::now() < deadline,
                "missing {method} request {count}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn control(&self, action: &str) {
        self.runtime
            .set_session_config_option(AgentSessionSetConfigOptionRequest {
                agent_id: "codex".into(),
                session_id: "session".into(),
                config_id: "control".into(),
                value: ConfigOptionCurrentValue::id(action),
                diagnostic_operation_id: None,
            })
            .unwrap();
    }

    fn wait_for_delivery_failure(&self) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !self.sink.events().iter().any(|event| {
            matches!(event,
            AgentEvent::Activity { title, .. } if title == "Message delivery was not confirmed")
        }) {
            assert!(
                Instant::now() < deadline,
                "delivery failure must be visible"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self
            .runtime
            .close_session(&crate::agent::AgentSessionKey::new("codex", "session"));
    }
}

#[test]
fn advertised_steering_injects_without_finishing_the_prompt() {
    let Some(fixture) = Fixture::new("injected") else {
        return;
    };
    let result = fixture.start();
    fixture.runtime.steer(fixture.prompt("correction")).unwrap();
    fixture.wait_for("_session/steering", 1);
    fixture.control("barrier");
    assert!(result.try_recv().is_err(), "acceptance is not completion");
    assert_eq!(fixture.calls("session/prompt").len(), 1);
    let requests = fixture.calls("_session/steering");
    assert_eq!(requests[0]["params"]["prompt"][0]["text"], "correction");
    assert_eq!(
        requests[0]["params"]["_meta"]["steering"]["idleBehavior"],
        "promptRequired"
    );
    fixture.control("finish");
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        AgentPromptOutcome::EndTurn
    );
}

#[test]
fn unadvertised_steering_keeps_the_existing_prompt_fallback() {
    for mode in [
        "unsupported",
        "disabled",
        "invalid_capability",
        "nested_capability",
    ] {
        assert_prompt_fallback(mode);
    }
}

fn assert_prompt_fallback(mode: &str) {
    let Some(fixture) = Fixture::new(mode) else {
        return;
    };
    let result = fixture.start();
    fixture.runtime.steer(fixture.prompt("correction")).unwrap();
    fixture.wait_for("session/prompt", 2);
    assert!(fixture.calls("_session/steering").is_empty());
    fixture.control("finish");
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        AgentPromptOutcome::EndTurn
    );
}

#[test]
fn late_steering_keeps_the_turn_open_until_the_tracked_continuation_finishes() {
    assert_late_steering("prompt_required");
}

#[test]
fn continuation_ignores_a_late_response_from_its_original_prompt() {
    assert_late_steering("prompt_required_early");
}

fn assert_late_steering(mode: &str) {
    let Some(fixture) = Fixture::new(mode) else {
        return;
    };
    let result = fixture.start();
    fixture
        .runtime
        .steer(fixture.prompt("late correction"))
        .unwrap();
    fixture.wait_for("_session/steering", 1);
    fixture.wait_for("session/prompt", 2);
    fixture.control("barrier");
    assert!(
        result.try_recv().is_err(),
        "the old end_turn cannot finish the continuation"
    );
    fixture.control("finish");
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        AgentPromptOutcome::EndTurn
    );
}

#[test]
fn method_not_found_falls_back_and_disables_the_extension_for_the_attachment() {
    let Some(fixture) = Fixture::new("method_not_found") else {
        return;
    };
    let result = fixture.start();
    fixture
        .runtime
        .steer(fixture.prompt("first correction"))
        .unwrap();
    fixture.wait_for("session/prompt", 2);
    fixture
        .runtime
        .steer(fixture.prompt("second correction"))
        .unwrap();
    fixture.wait_for("session/prompt", 3);
    assert_eq!(fixture.calls("_session/steering").len(), 1);
    fixture.control("finish");
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        AgentPromptOutcome::EndTurn
    );
}

#[test]
fn failed_or_ambiguous_steering_never_replays_the_message() {
    for mode in ["failed", "rpc_error", "malformed", "started_new_turn"] {
        let Some(fixture) = Fixture::new(mode) else {
            return;
        };
        let result = fixture.start();
        fixture.runtime.steer(fixture.prompt("correction")).unwrap();
        fixture.wait_for_delivery_failure();
        fixture.control("barrier");
        assert_eq!(fixture.calls("session/prompt").len(), 1, "{mode}");
        assert!(
            result.try_recv().is_err(),
            "failure does not finish the original prompt: {mode}"
        );
        fixture.control("finish");
        assert_eq!(
            result
                .recv_timeout(Duration::from_secs(3))
                .unwrap()
                .unwrap(),
            AgentPromptOutcome::EndTurn
        );
    }
}

#[test]
fn extension_acceptance_does_not_suppress_primary_cancellation() {
    let Some(fixture) = Fixture::new("injected") else {
        return;
    };
    let result = fixture.start();
    fixture.runtime.steer(fixture.prompt("correction")).unwrap();
    fixture.wait_for("_session/steering", 1);
    fixture.control("finish_cancelled");
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        AgentPromptOutcome::Cancelled
    );
}

#[test]
fn cancellation_does_not_wait_for_a_stalled_steering_acknowledgment() {
    let Some(fixture) = Fixture::new("held") else {
        return;
    };
    let result = fixture.start();
    fixture.runtime.steer(fixture.prompt("correction")).unwrap();
    fixture.wait_for("_session/steering", 1);
    fixture
        .runtime
        .cancel_session(&crate::agent::AgentSessionKey::new("codex", "session"))
        .unwrap();
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        AgentPromptOutcome::Cancelled
    );
    fixture.control("release");
    assert_eq!(fixture.calls("session/prompt").len(), 1);
}

#[test]
fn concurrent_steering_is_delivered_in_order() {
    let Some(fixture) = Fixture::new("held") else {
        return;
    };
    let result = fixture.start();
    fixture.runtime.steer(fixture.prompt("first")).unwrap();
    fixture.wait_for("_session/steering", 1);
    fixture.runtime.steer(fixture.prompt("second")).unwrap();
    fixture.control("barrier");
    assert_eq!(fixture.calls("_session/steering").len(), 1);
    fixture.control("release");
    fixture.wait_for("_session/steering", 2);
    fixture.control("release");
    fixture.control("finish");
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        AgentPromptOutcome::EndTurn
    );
    let calls = fixture.calls("_session/steering");
    assert_eq!(calls[0]["params"]["prompt"][0]["text"], "first");
    assert_eq!(calls[1]["params"]["prompt"][0]["text"], "second");
}

const AGENT: &str = r#"
import json, sys
log, mode = sys.argv[1:]
pending = []
held = []
def send(message):
    print(json.dumps(dict(jsonrpc='2.0', **message)), flush=True)
def reply(request, result):
    send(dict(id=request['id'], result=result))
def finish(reason='end_turn'):
    while pending:
        reply(pending.pop(0), dict(stopReason=reason))
def options():
    return [dict(id='control', name='Control', type='select', currentValue='barrier',
        options=[dict(value=value, name=value) for value in ['barrier', 'finish', 'finish_cancelled', 'release']])]
for line in sys.stdin:
    request = json.loads(line)
    with open(log, 'a') as output:
        output.write(json.dumps(request) + '\n')
    method = request.get('method')
    if method == 'initialize':
        result = dict(protocolVersion=1, agentCapabilities=dict(sessionCapabilities=dict(close={})))
        if mode != 'unsupported':
            result['_meta'] = dict(steering=dict(supported=True))
        if mode == 'disabled':
            result['_meta']['steering']['supported'] = False
        if mode == 'invalid_capability':
            result['_meta']['steering']['supported'] = 'true'
        if mode == 'nested_capability':
            result['agentCapabilities']['_meta'] = result.pop('_meta')
        reply(request, result)
    elif method == 'session/new':
        reply(request, dict(sessionId='session', configOptions=options()))
    elif method == 'session/prompt':
        pending.append(request)
    elif method == '_session/steering':
        if mode == 'prompt_required':
            finish()
            reply(request, dict(outcome='promptRequired', reason='noRunningTurn'))
        elif mode == 'prompt_required_early':
            reply(request, dict(outcome='promptRequired', reason='noRunningTurn'))
            finish()
        elif mode in ['method_not_found', 'rpc_error']:
            send(dict(id=request['id'], error=dict(code=-32601 if mode == 'method_not_found' else -32603, message='fixture error')))
        elif mode == 'failed':
            reply(request, dict(outcome='failed'))
        elif mode == 'malformed':
            reply(request, {})
        elif mode == 'started_new_turn':
            reply(request, dict(outcome='startedNewTurn'))
        elif mode == 'held':
            held.append(request)
        else:
            reply(request, dict(outcome='injected'))
    elif method == 'session/set_config_option':
        if request['params']['value'] == 'finish':
            finish()
        if request['params']['value'] == 'finish_cancelled':
            finish('cancelled')
        if request['params']['value'] == 'release':
            while held:
                reply(held.pop(0), dict(outcome='injected'))
        reply(request, dict(configOptions=options()))
    elif method == 'session/cancel':
        finish('cancelled')
    elif method == 'session/close':
        finish('cancelled')
        reply(request, {})
    elif 'id' in request:
        send(dict(id=request['id'], error=dict(code=-32601, message='Unknown method')))
"#;
