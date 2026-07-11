use super::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::agent::acp_active_session_manager::AcpActiveSessionManager;
use crate::agent::acp_auth_method_cache::AcpAuthMethodCache;
use crate::agent::registry::AgentRegistry;
use crate::agent::{AgentSecretResolver, AgentSessionSetConfigOptionRequest, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

fn fixture_runtime(
    temp: &tempfile::TempDir,
    session_id: &str,
) -> Option<(AcpAgentRuntime, PathBuf)> {
    fixture_runtime_with_secret_env(temp, session_id, Vec::new())
}

fn fixture_runtime_with_secret_env(
    temp: &tempfile::TempDir,
    session_id: &str,
    secret_env: Vec<String>,
) -> Option<(AcpAgentRuntime, PathBuf)> {
    if !python3_available() {
        eprintln!("skipping ACP active-session runtime fixture: python3 not found");
        return None;
    }

    let script_path = temp.path().join("fixture_agent.py");
    let log_path = temp.path().join("fixture.log");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    let runtime = AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "python3".to_string(),
        args: vec![script_path.to_string_lossy().to_string()],
        env: vec![
            (
                "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                log_path.to_string_lossy().to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                session_id.to_string(),
            ),
        ],
        secret_env,
    });
    Some((runtime, log_path))
}

fn fixture_runtime_with_prompt_mode(
    temp: &tempfile::TempDir,
    session_id: &str,
    prompt_mode: &str,
) -> Option<(AcpAgentRuntime, PathBuf)> {
    if !python3_available() {
        eprintln!("skipping ACP active-session runtime fixture: python3 not found");
        return None;
    }
    let script_path = temp.path().join("fixture_agent.py");
    let log_path = temp.path().join("fixture.log");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    Some((
        AcpAgentRuntime::new(AcpAgentConfig {
            agent_id: "codex".to_string(),
            command: "python3".to_string(),
            args: vec![script_path.to_string_lossy().to_string()],
            env: vec![
                (
                    "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                    log_path.to_string_lossy().to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                    session_id.to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                    prompt_mode.to_string(),
                ),
            ],
            secret_env: Vec::new(),
        }),
        log_path,
    ))
}

fn fixture_manager(
    temp: &tempfile::TempDir,
    session_id: &str,
) -> Option<(AcpActiveSessionManager, PathBuf)> {
    if !python3_available() {
        eprintln!("skipping ACP active-session manager fixture: python3 not found");
        return None;
    }

    let script_path = temp.path().join("fixture_agent.py");
    let log_path = temp.path().join("fixture.log");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    let manager = AcpActiveSessionManager::new(
        AgentRegistry::codex(AcpAgentConfig {
            agent_id: "codex".to_string(),
            command: "python3".to_string(),
            args: vec![script_path.to_string_lossy().to_string()],
            env: vec![
                (
                    "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                    log_path.to_string_lossy().to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                    session_id.to_string(),
                ),
            ],
            secret_env: Vec::new(),
        }),
        HostBridge::disabled(),
        AcpAuthMethodCache::default(),
    );
    Some((manager, log_path))
}

fn python3_available() -> bool {
    Command::new("python3").arg("--version").output().is_ok()
}

fn cwd_string() -> String {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("/"))
        .to_string_lossy()
        .to_string()
}

fn start_request(task_id: &str, cwd: String) -> AgentSessionStart {
    AgentSessionStart {
        agent_id: "codex".to_string(),
        task_id: task_id.to_string(),
        cwd,
        model_id: None,
        config_options: None,
        config_option_policy: crate::agent::ConfigOptionPolicy::Strict,
        context: Vec::new(),
        cancellation: TurnCancellation::new(),
        secret_resolver: None,
    }
}

#[test]
fn draft_recovery_ignores_config_missing_from_fresh_agent_catalog() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, _log_path)) = fixture_runtime(&temp, "reconciled-session") else {
        return;
    };
    let mut request = start_request("task-stale-config", cwd_string());
    request.config_options = Some(serde_json::json!({ "mode": "full-access" }));
    request.config_option_policy = crate::agent::ConfigOptionPolicy::ReconcileWithAgentDefaults;

    let session = runtime
        .start_session(request)
        .expect("reconcile stale option");

    assert_eq!(session.session_id, "reconciled-session");
    assert!(session.config_options.is_empty());
}

fn inactive_runtime() -> AcpAgentRuntime {
    AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "openaide-test-agent".to_string(),
        args: Vec::new(),
        env: Vec::new(),
        secret_env: Vec::new(),
    })
}

fn read_fixture_methods(log_path: &Path) -> Vec<String> {
    fs::read_to_string(log_path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn wait_for_method(log_path: &Path, method: &str) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        if read_fixture_methods(log_path)
            .iter()
            .any(|seen| seen == method)
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for fixture method {method}");
}

fn wait_for_method_count(log_path: &Path, method: &str, expected_count: usize) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        let count = read_fixture_methods(log_path)
            .iter()
            .filter(|seen| seen.as_str() == method)
            .count();
        if count >= expected_count {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {expected_count} fixture calls to {method}");
}

#[derive(Debug)]
struct ObservedTerminalHostRequest {
    method: String,
    session_id: String,
    terminal_id: Option<String>,
}

fn run_terminal_host(
    host_bridge: HostBridge,
    requests: mpsc::Receiver<crate::protocol::host::HostRequest>,
    expected_releases: usize,
) -> (
    mpsc::Receiver<ObservedTerminalHostRequest>,
    std::thread::JoinHandle<()>,
) {
    let (observed_tx, observed_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut created = 0;
        let mut released = 0;
        loop {
            let request = requests
                .recv_timeout(Duration::from_secs(3))
                .expect("terminal host request");
            let params = request.params.as_ref().expect("terminal host params");
            let observed = ObservedTerminalHostRequest {
                method: request.method.clone(),
                session_id: params["sessionId"].as_str().unwrap_or_default().to_string(),
                terminal_id: params["terminalId"].as_str().map(str::to_string),
            };
            observed_tx.send(observed).expect("observe host request");
            let result = match request.method.as_str() {
                "terminal/create" => {
                    created += 1;
                    serde_json::json!({ "terminalId": format!("fixture-terminal-{created}") })
                }
                "terminal/wait_for_exit" => serde_json::json!({ "exitCode": 0 }),
                _ => serde_json::Value::Null,
            };
            assert!(host_bridge.try_handle_response(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": result,
            })));
            if request.method == "terminal/release" {
                released += 1;
                if released == expected_releases {
                    break;
                }
            }
        }
    });
    (observed_rx, handle)
}

fn expect_terminal_host_request(
    observed: &mpsc::Receiver<ObservedTerminalHostRequest>,
    method: &str,
) -> ObservedTerminalHostRequest {
    let request = observed
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|_| panic!("expected {method} host request"));
    assert_eq!(request.method, method);
    request
}

fn fixture_agent_script() -> &'static str {
    r#"import json
import os
import sys

log_path = os.environ["OPENAIDE_ACP_FIXTURE_LOG"]
session_id = os.environ.get("OPENAIDE_ACP_FIXTURE_SESSION", "fixture-session")
prompt_mode = os.environ.get("OPENAIDE_ACP_FIXTURE_PROMPT_MODE", "")
pending_prompt_ids = []
next_session_number = 0
closed_session_count = 0

def log(method):
    with open(log_path, "a", encoding="utf-8") as file:
        file.write(method + "\n")

if "OPENAIDE_SECRET_TEST" in os.environ:
    log("secret:" + os.environ["OPENAIDE_SECRET_TEST"])

def respond_id(message_id, result):
    sys.stdout.write(json.dumps({
        "jsonrpc": "2.0",
        "id": message_id,
        "result": result,
    }) + "\n")
    sys.stdout.flush()

def respond(message, result):
    respond_id(message.get("id"), result)

def notify(method, params):
    sys.stdout.write(json.dumps({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    }) + "\n")
    sys.stdout.flush()

def notify_text_chunk(text):
    notify("session/update", {
        "sessionId": session_id,
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": {
                "type": "text",
                "text": text,
            },
        },
    })

def notify_title(title):
    notify("session/update", {
        "sessionId": session_id,
        "update": {
            "sessionUpdate": "session_info_update",
            "title": title,
        },
    })

def request_terminal(request_id):
    sys.stdout.write(json.dumps({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "terminal/create",
        "params": {
            "sessionId": session_id,
            "command": "long-running-fixture",
        },
    }) + "\n")
    sys.stdout.flush()

def initialize_result():
    return {
        "protocolVersion": 1,
        "agentCapabilities": {
            "loadSession": True,
            "sessionCapabilities": {
                "close": {},
                "delete": {},
                "list": {},
            },
        },
        "authMethods": [],
    }

for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method is not None:
        log(method)
    if method is None:
        log("terminal/create.response")
    elif method == "initialize":
        respond(message, initialize_result())
    elif method == "session/new":
        next_session_number += 1
        if prompt_mode == "host_terminal_during_new_hang":
            request_terminal("startup-terminal-create")
        elif session_id == "__counter__":
            respond(message, {"sessionId": f"counter-session-{next_session_number}"})
        elif session_id == "__second_new_error__" and next_session_number == 2:
            sys.stdout.write(json.dumps({
                "jsonrpc": "2.0",
                "id": message.get("id"),
                "error": {"code": -32000, "message": "second session rejected"},
            }) + "\n")
            sys.stdout.flush()
        else:
            respond(message, {"sessionId": session_id})
            if prompt_mode == "title_after_new":
                notify_title("Agent generated title")
    elif method == "session/load":
        respond(message, {"configOptions": []})
    elif method == "session/list":
        respond(message, {"sessions": []})
    elif method == "session/prompt":
        if prompt_mode == "host_terminal_wait_for_cancel":
            pending_prompt_ids.append(message.get("id"))
            request_terminal("prompt-terminal-create-1")
            request_terminal("prompt-terminal-create-2")
        elif prompt_mode == "coalesced_steering_response":
            pending_prompt_ids.append(message.get("id"))
            if len(pending_prompt_ids) == 2:
                notify_text_chunk("steering applied")
                respond_id(pending_prompt_ids.pop(0), {"stopReason": "end_turn"})
        elif prompt_mode == "wait_for_cancel":
            pending_prompt_ids.append(message.get("id"))
        elif prompt_mode == "late_text_after_response":
            respond(message, {"stopReason": "end_turn"})
            notify_text_chunk("late response text")
        elif prompt_mode == "title_during_prompt":
            notify_title("Title from active turn")
            respond(message, {"stopReason": "end_turn"})
        else:
            respond(message, {"stopReason": "end_turn"})
    elif method == "session/set_config_option":
        params = message.get("params", {})
        config_id = params.get("configId", "model")
        value = params.get("value", "gpt-5")
        respond(message, {
            "configOptions": [{
                "id": config_id,
                "name": "Model",
                "type": "select",
                "currentValue": value,
                "options": [
                    {"value": "gpt-5", "name": "GPT 5"},
                    {"value": "gpt-5.5", "name": "GPT 5.5"}
                ],
            }],
        })
    elif method == "session/cancel":
        while pending_prompt_ids:
            respond_id(pending_prompt_ids.pop(0), {"stopReason": "cancelled"})
    elif method == "session/close":
        respond(message, {})
        if session_id == "__counter__":
            closed_session_count += 1
            if closed_session_count >= 2 and closed_session_count >= next_session_number:
                break
        elif session_id == "duplicate-session":
            closed_session_count += 1
            if closed_session_count >= 2:
                break
        else:
            break
    elif method == "session/delete":
        respond(message, {})
        break
    else:
        sys.stdout.write(json.dumps({
            "jsonrpc": "2.0",
            "id": message.get("id"),
            "error": {"code": -32601, "message": "unknown method"},
        }) + "\n")
        sys.stdout.flush()
"#
}

struct StaticSecretResolver {
    values: HashMap<String, String>,
}

impl AgentSecretResolver for StaticSecretResolver {
    fn resolve_secret_env(
        &self,
        _agent_id: &str,
        names: &[String],
    ) -> Result<HashMap<String, String>, RuntimeError> {
        names
            .iter()
            .map(|name| {
                self.values
                    .get(name)
                    .map(|value| (name.clone(), value.clone()))
                    .ok_or_else(|| RuntimeError::NotReady(format!("missing secret env {name}")))
            })
            .collect()
    }
}

#[test]
fn start_prompt_and_close_dispatch_through_active_sessions() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "runtime-session") else {
        return;
    };

    let session = runtime
        .start_session(start_request("task-runtime-start", cwd_string()))
        .expect("start session");
    assert_eq!(session.session_id, "runtime-session");

    runtime
        .prompt(
            AgentPrompt {
                task_id: "task-runtime-start".to_string(),
                session_id: session.session_id.clone(),
                text: "hello".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(CapturingEventSink::default()),
        )
        .expect("prompt");
    runtime
        .close_session(&session.session_id)
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/prompt",
            "session/close"
        ]
    );
}

#[test]
fn listing_sessions_does_not_create_a_native_session() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "unused-list-session") else {
        return;
    };

    runtime
        .list_sessions(AgentListSessionsRequest {
            agent_id: "codex".to_string(),
            cwd: cwd_string(),
            cursor: None,
        })
        .expect("list sessions");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/list"]
    );
}

#[test]
fn prompt_delivers_text_update_sent_after_prompt_response() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "late-text-session") else {
        return;
    };
    let script_path = temp.path().join("fixture_agent.py");
    let runtime = AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "python3".to_string(),
        args: vec![script_path.to_string_lossy().to_string()],
        env: vec![
            (
                "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                log_path.to_string_lossy().to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                "late-text-session".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                "late_text_after_response".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    });

    let capture = Arc::new(CapturingEventSink::default());
    let session = runtime
        .start_session(start_request("task-late-text", cwd_string()))
        .expect("start session");
    runtime
        .prompt(
            AgentPrompt {
                task_id: "task-late-text".to_string(),
                session_id: session.session_id.clone(),
                text: "hello".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            capture.clone(),
        )
        .expect("prompt");
    runtime
        .close_session(&session.session_id)
        .expect("close session");

    let events = capture.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        AgentEvent::TextChunk { text, .. } => assert_eq!(text, "late response text"),
        other => panic!("expected late text event, got {other:?}"),
    }
}

#[test]
fn start_session_passes_resolved_secret_env_to_acp_process() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime_with_secret_env(
        &temp,
        "secret-session",
        vec!["OPENAIDE_SECRET_TEST".to_string()],
    ) else {
        return;
    };
    let mut request = start_request("task-secret-env", cwd_string());
    request.secret_resolver = Some(Arc::new(StaticSecretResolver {
        values: HashMap::from([(
            "OPENAIDE_SECRET_TEST".to_string(),
            "resolved-secret".to_string(),
        )]),
    }));

    let session = runtime.start_session(request).expect("start session");
    assert_eq!(session.session_id, "secret-session");
    wait_for_method(&log_path, "secret:resolved-secret");
    runtime.close_session(&session.session_id).unwrap();
}

#[test]
fn resume_and_attach_session_event_sink_use_active_session_registry() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "resumable-session") else {
        return;
    };

    let session = runtime
        .start_session(start_request("task-resume", cwd_string()))
        .expect("start session");
    let resumed = runtime
        .resume_session(AgentSessionResume {
            agent_id: "codex".to_string(),
            task_id: "task-resume".to_string(),
            session_id: session.session_id.clone(),
            cwd: cwd_string(),
            model_id: None,
            cancellation: TurnCancellation::new(),
        })
        .expect("resume active session");
    assert_eq!(resumed.session_id, session.session_id);

    runtime
        .attach_session_event_sink(
            &session.session_id,
            Arc::new(CapturingSessionSink::default()),
        )
        .expect("attach session sink");
    runtime
        .close_session(&session.session_id)
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "session/close"]
    );
}

#[test]
fn session_title_update_before_sink_attachment_is_delivered() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, _log_path)) =
        fixture_runtime_with_prompt_mode(&temp, "metadata-session", "title_after_new")
    else {
        return;
    };
    let session = runtime
        .start_session(start_request("task-metadata", cwd_string()))
        .expect("start session");
    let sink = Arc::new(CapturingSessionSink::default());

    runtime
        .attach_session_event_sink(&session.session_id, sink.clone())
        .expect("attach session sink");

    let started = Instant::now();
    while sink.metadata_updates().is_empty() && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        sink.metadata_updates(),
        vec![AgentSessionMetadataUpdate {
            title: AgentMetadataField::Value("Agent generated title".to_string()),
            updated_at: AgentMetadataField::Unchanged,
        }]
    );
    runtime.close_session(&session.session_id).unwrap();
}

#[test]
fn session_title_update_during_prompt_is_delivered_to_session_sink() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, _log_path)) =
        fixture_runtime_with_prompt_mode(&temp, "active-metadata-session", "title_during_prompt")
    else {
        return;
    };
    let session = runtime
        .start_session(start_request("task-active-metadata", cwd_string()))
        .expect("start session");
    let sink = Arc::new(CapturingSessionSink::default());
    runtime
        .attach_session_event_sink(&session.session_id, sink.clone())
        .expect("attach session sink");

    runtime
        .prompt(
            AgentPrompt {
                task_id: "task-active-metadata".to_string(),
                session_id: session.session_id.clone(),
                text: "do work".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(CapturingEventSink::default()),
        )
        .expect("prompt");

    assert_eq!(
        sink.metadata_updates(),
        vec![AgentSessionMetadataUpdate {
            title: AgentMetadataField::Value("Title from active turn".to_string()),
            updated_at: AgentMetadataField::Unchanged,
        }]
    );
    runtime.close_session(&session.session_id).unwrap();
}

#[test]
fn inactive_session_registry_reports_stable_missing_session_errors() {
    let runtime = inactive_runtime();

    let resume_error = match runtime.resume_session(AgentSessionResume {
        agent_id: "codex".to_string(),
        task_id: "task-missing-resume".to_string(),
        session_id: "missing-session".to_string(),
        cwd: cwd_string(),
        model_id: None,
        cancellation: TurnCancellation::new(),
    }) {
        Ok(session) => panic!(
            "missing resume unexpectedly succeeded: {}",
            session.session_id
        ),
        Err(error) => error.to_string(),
    };
    assert_eq!(
        resume_error,
        "capability missing: acp_session_resume_after_runtime_restart"
    );

    let attach_error = runtime
        .attach_session_event_sink("missing-session", Arc::new(CapturingSessionSink::default()))
        .expect_err("missing attach should fail")
        .to_string();
    assert_eq!(attach_error, "runtime not ready: ACP session is not active");

    let prompt_error = runtime
        .prompt(
            AgentPrompt {
                task_id: "task-missing-prompt".to_string(),
                session_id: "missing-session".to_string(),
                text: "hello".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(CapturingEventSink::default()),
        )
        .expect_err("missing prompt should fail")
        .to_string();
    assert_eq!(prompt_error, "runtime not ready: ACP session is not active");

    let delete_error = runtime
        .delete_session(AgentSessionDelete {
            session_id: "missing-session".to_string(),
        })
        .expect_err("missing delete should fail")
        .to_string();
    assert_eq!(delete_error, "runtime not ready: ACP session is not active");
}

#[test]
fn inactive_session_cancel_and_close_are_idempotent() {
    let runtime = inactive_runtime();

    runtime
        .cancel_session("missing-session")
        .expect("missing cancel remains idempotent");
    runtime
        .close_session("missing-session")
        .expect("missing close remains idempotent");
}

#[test]
fn cancel_session_dispatches_to_active_prompt() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "cancel-session") else {
        return;
    };
    let script_path = temp.path().join("fixture_agent.py");
    let runtime = AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "python3".to_string(),
        args: vec![script_path.to_string_lossy().to_string()],
        env: vec![
            (
                "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                log_path.to_string_lossy().to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                "cancel-session".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                "wait_for_cancel".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    });

    let runtime = Arc::new(runtime);
    let session = runtime
        .start_session(start_request("task-cancel", cwd_string()))
        .expect("start session");
    let prompt_session_id = session.session_id.clone();
    let prompt_handle = std::thread::spawn({
        let runtime_for_prompt = runtime.clone();
        move || {
            runtime_for_prompt.prompt(
                AgentPrompt {
                    task_id: "task-cancel".to_string(),
                    session_id: prompt_session_id,
                    text: "cancel me".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });

    wait_for_method(&log_path, "session/prompt");
    runtime
        .cancel_session(&session.session_id)
        .expect("cancel session");
    let prompt_result = prompt_handle.join().expect("prompt thread");
    prompt_result.expect("prompt should finish after cancel");
    runtime
        .close_session(&session.session_id)
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/prompt",
            "session/cancel",
            "session/close"
        ]
    );
}

#[test]
fn cancel_session_kills_and_releases_owned_host_terminals_before_returning() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        return;
    }
    let script_path = temp.path().join("fixture_agent.py");
    let log_path = temp.path().join("fixture.log");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    let (host_bridge, host_requests) = HostBridge::channel_with_timeout(Duration::from_secs(1));
    let (observed, host_handle) = run_terminal_host(host_bridge.clone(), host_requests, 2);
    let runtime = Arc::new(AcpAgentRuntime::new_with_host(
        AcpAgentConfig {
            agent_id: "codex".to_string(),
            command: "python3".to_string(),
            args: vec![script_path.to_string_lossy().to_string()],
            env: vec![
                (
                    "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                    log_path.to_string_lossy().to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                    "terminal-cancel-session".to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                    "host_terminal_wait_for_cancel".to_string(),
                ),
            ],
            secret_env: Vec::new(),
        },
        host_bridge,
    ));
    let session = runtime
        .start_session(start_request("task-terminal-cancel", cwd_string()))
        .expect("start session");
    let prompt_session_id = session.session_id.clone();
    let prompt_handle = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    task_id: "task-terminal-cancel".to_string(),
                    session_id: prompt_session_id,
                    text: "run a terminal".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });

    for _ in 0..2 {
        let create = expect_terminal_host_request(&observed, "terminal/create");
        assert_eq!(create.session_id, session.session_id);
    }
    wait_for_method_count(&log_path, "terminal/create.response", 2);

    runtime
        .cancel_session(&session.session_id)
        .expect("cancel session");

    let mut killed = HashSet::new();
    for _ in 0..2 {
        let request = expect_terminal_host_request(&observed, "terminal/kill");
        assert_eq!(request.session_id, session.session_id);
        assert!(killed.insert(request.terminal_id.expect("killed terminal id")));
    }
    let mut waited = HashSet::new();
    for _ in 0..2 {
        let wait = expect_terminal_host_request(&observed, "terminal/wait_for_exit");
        assert_eq!(wait.session_id, session.session_id);
        assert!(waited.insert(wait.terminal_id.expect("waited terminal id")));
    }
    let mut released = HashSet::new();
    for _ in 0..2 {
        let release = expect_terminal_host_request(&observed, "terminal/release");
        assert_eq!(release.session_id, session.session_id);
        assert!(released.insert(release.terminal_id.expect("released terminal id")));
    }
    let expected_terminal_ids = HashSet::from([
        "fixture-terminal-1".to_string(),
        "fixture-terminal-2".to_string(),
    ]);
    assert_eq!(killed, expected_terminal_ids);
    assert_eq!(waited, expected_terminal_ids);
    assert_eq!(released, expected_terminal_ids);
    host_handle.join().expect("terminal host thread");
    prompt_handle
        .join()
        .expect("prompt thread")
        .expect("prompt cancelled");
    runtime
        .close_session(&session.session_id)
        .expect("close session");
}

#[test]
fn timed_out_session_start_cleans_up_terminals_created_during_partial_start() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        return;
    }
    let script_path = temp.path().join("fixture_agent.py");
    let log_path = temp.path().join("fixture.log");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    let (host_bridge, host_requests) = HostBridge::channel_with_timeout(Duration::from_secs(1));
    let (observed, host_handle) = run_terminal_host(host_bridge.clone(), host_requests, 1);
    let mut manager = AcpActiveSessionManager::new(
        AgentRegistry::codex(AcpAgentConfig {
            agent_id: "codex".to_string(),
            command: "python3".to_string(),
            args: vec![script_path.to_string_lossy().to_string()],
            env: vec![
                (
                    "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                    log_path.to_string_lossy().to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                    "partial-start-session".to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                    "host_terminal_during_new_hang".to_string(),
                ),
            ],
            secret_env: Vec::new(),
        }),
        host_bridge,
        AcpAuthMethodCache::default(),
    );
    manager.with_start_timeout(Duration::from_millis(200));

    let error = match manager.start_session(start_request("task-partial-terminal", cwd_string())) {
        Ok(_) => panic!("session start should time out"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("timed out"));

    let create = expect_terminal_host_request(&observed, "terminal/create");
    assert_eq!(create.session_id, "partial-start-session");
    let kill = expect_terminal_host_request(&observed, "terminal/kill");
    assert_eq!(kill.terminal_id.as_deref(), Some("fixture-terminal-1"));
    let wait = expect_terminal_host_request(&observed, "terminal/wait_for_exit");
    assert_eq!(wait.terminal_id.as_deref(), Some("fixture-terminal-1"));
    let release = expect_terminal_host_request(&observed, "terminal/release");
    assert_eq!(release.terminal_id.as_deref(), Some("fixture-terminal-1"));
    host_handle.join().expect("terminal host thread");
}

#[test]
fn steering_prompt_dispatches_while_prior_prompt_is_running() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "steer-session") else {
        return;
    };
    let script_path = temp.path().join("fixture_agent.py");
    let runtime = Arc::new(AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "python3".to_string(),
        args: vec![script_path.to_string_lossy().to_string()],
        env: vec![
            (
                "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                log_path.to_string_lossy().to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                "steer-session".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                "wait_for_cancel".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    }));

    let session = runtime
        .start_session(start_request("task-steer", cwd_string()))
        .expect("start session");
    let first_session_id = session.session_id.clone();
    let first_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    task_id: "task-steer".to_string(),
                    session_id: first_session_id,
                    text: "start work".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });
    wait_for_method_count(&log_path, "session/prompt", 1);

    let steer_session_id = session.session_id.clone();
    let steer_cancellation = TurnCancellation::new();
    let steer_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        let cancellation = steer_cancellation.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    task_id: "task-steer".to_string(),
                    session_id: steer_session_id,
                    text: "steer now".to_string(),
                    attachments: Vec::new(),
                    cancellation,
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        read_fixture_methods(&log_path)
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        2,
        "steering must reach the Agent without waiting for the active ACP request"
    );
    steer_cancellation.cancel();
    runtime
        .cancel_session(&session.session_id)
        .expect("cancel session");
    first_prompt
        .join()
        .expect("first prompt thread")
        .expect("first prompt should finish after cancel");
    steer_prompt
        .join()
        .expect("steer prompt thread")
        .expect("steer prompt should finish after cancel");
    runtime
        .close_session(&session.session_id)
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/prompt",
            "session/prompt",
            "session/cancel",
            "session/close"
        ]
    );
}

#[test]
fn coalesced_steering_response_finishes_logical_prompt_without_cancel() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime_with_prompt_mode(
        &temp,
        "coalesced-steer-session",
        "coalesced_steering_response",
    ) else {
        return;
    };
    let runtime = Arc::new(runtime);
    let session = runtime
        .start_session(start_request("task-coalesced-steer", cwd_string()))
        .expect("start session");
    let (result_tx, result_rx) = mpsc::channel();

    let primary_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        let result_tx = result_tx.clone();
        let session_id = session.session_id.clone();
        move || {
            let result = runtime.prompt(
                AgentPrompt {
                    task_id: "task-coalesced-steer".to_string(),
                    session_id,
                    text: "start work".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            );
            let _ = result_tx.send(("primary", result));
        }
    });
    wait_for_method_count(&log_path, "session/prompt", 1);

    let steering_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        let result_tx = result_tx.clone();
        let session_id = session.session_id.clone();
        move || {
            let result = runtime.prompt(
                AgentPrompt {
                    task_id: "task-coalesced-steer".to_string(),
                    session_id,
                    text: "steer now".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            );
            let _ = result_tx.send(("steering", result));
        }
    });
    wait_for_method_count(&log_path, "session/prompt", 2);

    let first = result_rx.recv_timeout(Duration::from_millis(500));
    let second = result_rx.recv_timeout(Duration::from_millis(500));
    if first.is_err() || second.is_err() {
        runtime
            .cancel_session(&session.session_id)
            .expect("clean up stuck coalesced prompt");
    }
    let mut completed = [
        first.expect("first logical prompt completion"),
        second.expect("second logical prompt completion"),
    ];
    completed.sort_by_key(|(name, _)| *name);
    assert!(completed.into_iter().all(|(_, result)| result.is_ok()));
    assert!(!read_fixture_methods(&log_path)
        .iter()
        .any(|method| method == "session/cancel"));

    primary_prompt.join().expect("primary prompt thread");
    steering_prompt.join().expect("steering prompt thread");
    runtime
        .close_session(&session.session_id)
        .expect("close session");
}

#[test]
fn set_config_option_dispatches_while_prompt_is_running() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "config-session") else {
        return;
    };
    let script_path = temp.path().join("fixture_agent.py");
    let runtime = Arc::new(AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "python3".to_string(),
        args: vec![script_path.to_string_lossy().to_string()],
        env: vec![
            (
                "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                log_path.to_string_lossy().to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                "config-session".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                "wait_for_cancel".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    }));

    let session = runtime
        .start_session(start_request("task-live-config", cwd_string()))
        .expect("start session");
    let prompt_session_id = session.session_id.clone();
    let prompt_handle = std::thread::spawn({
        let runtime_for_prompt = runtime.clone();
        move || {
            runtime_for_prompt.prompt(
                AgentPrompt {
                    task_id: "task-live-config".to_string(),
                    session_id: prompt_session_id,
                    text: "keep running".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });
    wait_for_method(&log_path, "session/prompt");

    let catalog = runtime
        .set_session_config_option(AgentSessionSetConfigOptionRequest {
            agent_id: "codex".to_string(),
            session_id: session.session_id.clone(),
            config_id: "model".to_string(),
            value: "gpt-5.5".to_string(),
        })
        .expect("set config option");
    assert_eq!(
        catalog.current_values().get("model"),
        Some(&"gpt-5.5".to_string())
    );

    runtime
        .cancel_session(&session.session_id)
        .expect("cancel session");
    prompt_handle
        .join()
        .expect("prompt thread")
        .expect("prompt cancelled cleanly");
    runtime
        .close_session(&session.session_id)
        .expect("close session");

    assert!(read_fixture_methods(&log_path)
        .iter()
        .any(|method| method == "session/set_config_option"));
}

#[test]
fn close_session_dispatches_while_prompt_is_running() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "close-running-prompt") else {
        return;
    };
    let script_path = temp.path().join("fixture_agent.py");
    let runtime = AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "python3".to_string(),
        args: vec![script_path.to_string_lossy().to_string()],
        env: vec![
            (
                "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                log_path.to_string_lossy().to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                "close-running-prompt".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                "wait_for_cancel".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    });

    let runtime = Arc::new(runtime);
    let session = runtime
        .start_session(start_request("task-close-running", cwd_string()))
        .expect("start session");
    let prompt_session_id = session.session_id.clone();
    let prompt_handle = std::thread::spawn({
        let runtime_for_prompt = runtime.clone();
        move || {
            runtime_for_prompt.prompt(
                AgentPrompt {
                    task_id: "task-close-running".to_string(),
                    session_id: prompt_session_id,
                    text: "close me".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });

    wait_for_method(&log_path, "session/prompt");
    runtime
        .close_session(&session.session_id)
        .expect("close running prompt");
    let prompt_error = prompt_handle
        .join()
        .expect("prompt thread")
        .expect_err("prompt should stop because the session closed")
        .to_string();
    assert_eq!(prompt_error, "runtime not ready: ACP session closed");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/prompt",
            "session/close"
        ]
    );
}

#[test]
fn duplicate_active_session_id_keeps_original_session_active() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "duplicate-session") else {
        return;
    };
    let cwd = cwd_string();

    let session = runtime
        .start_session(start_request("task-duplicate-one", cwd.clone()))
        .expect("first start");

    let error = match runtime.start_session(start_request("task-duplicate-two", cwd)) {
        Ok(session) => panic!(
            "duplicate start unexpectedly succeeded: {}",
            session.session_id
        ),
        Err(error) => error.to_string(),
    };

    assert_eq!(error, "invalid params: agent_session_id already active");
    runtime
        .close_session(&session.session_id)
        .expect("close original session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/new",
            "session/close",
            "session/close"
        ]
    );
}

#[test]
fn shared_process_open_failure_reports_without_start_timeout() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((mut manager, log_path)) = fixture_manager(&temp, "__second_new_error__") else {
        return;
    };
    manager.with_start_timeout(Duration::from_millis(750));
    let cwd = cwd_string();

    let first = manager
        .start_session(start_request("task-open-error-one", cwd.clone()))
        .expect("first start");
    let started = Instant::now();
    let error = match manager.start_session(start_request("task-open-error-two", cwd)) {
        Ok(session) => panic!(
            "second start unexpectedly succeeded: {}",
            session.session_id
        ),
        Err(error) => error.to_string(),
    };

    assert!(started.elapsed() < Duration::from_millis(500), "{error}");
    assert_eq!(
        error,
        "runtime not ready: ACP error: second session rejected"
    );

    manager
        .close_session(&first.session_id)
        .expect("close first session");
    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "session/new", "session/close"]
    );
}

#[test]
fn start_sessions_reuses_agent_process_for_same_agent() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "__counter__") else {
        return;
    };
    let cwd = cwd_string();

    let first = runtime
        .start_session(start_request("task-reuse-one", cwd.clone()))
        .expect("first start");
    let second = runtime
        .start_session(start_request("task-reuse-two", cwd))
        .expect("second start");

    assert_eq!(first.session_id, "counter-session-1");
    assert_eq!(second.session_id, "counter-session-2");
    runtime
        .close_session(&first.session_id)
        .expect("close first session");
    runtime
        .close_session(&second.session_id)
        .expect("close second session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/new",
            "session/close",
            "session/close"
        ]
    );
}

#[test]
fn start_session_while_existing_prompt_is_running_reuses_agent_process() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "running-prompt-session") else {
        return;
    };
    let script_path = temp.path().join("fixture_agent.py");
    let mut manager = AcpActiveSessionManager::new(
        AgentRegistry::codex(AcpAgentConfig {
            agent_id: "codex".to_string(),
            command: "python3".to_string(),
            args: vec![script_path.to_string_lossy().to_string()],
            env: vec![
                (
                    "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                    log_path.to_string_lossy().to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                    "__counter__".to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                    "wait_for_cancel".to_string(),
                ),
            ],
            secret_env: Vec::new(),
        }),
        HostBridge::disabled(),
        AcpAuthMethodCache::default(),
    );
    manager.with_start_timeout(Duration::from_millis(750));
    let runtime = Arc::new(manager);
    let cwd = cwd_string();

    let first = runtime
        .start_session(start_request("task-running-one", cwd.clone()))
        .expect("first start");
    let first_session_id = first.session_id.clone();
    let prompt_handle = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    task_id: "task-running-one".to_string(),
                    session_id: first_session_id,
                    text: "keep running".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });
    wait_for_method(&log_path, "session/prompt");

    let started = Instant::now();
    let second = runtime
        .start_session(start_request("task-running-two", cwd))
        .expect("second start should not wait for first prompt to finish");

    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(first.session_id, "counter-session-1");
    assert_eq!(second.session_id, "counter-session-2");

    runtime
        .cancel_session(&first.session_id)
        .expect("cancel first prompt");
    prompt_handle
        .join()
        .expect("prompt thread")
        .expect("prompt cancelled cleanly");
    runtime
        .close_session(&first.session_id)
        .expect("close first session");
    runtime
        .close_session(&second.session_id)
        .expect("close second session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/prompt",
            "session/new",
            "session/cancel",
            "session/close",
            "session/close"
        ]
    );
}

#[test]
fn start_and_load_sessions_reuse_agent_process_for_same_agent() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "__counter__") else {
        return;
    };
    let cwd = cwd_string();

    let started = runtime
        .start_session(start_request("task-reuse-start", cwd.clone()))
        .expect("start session");
    let loaded = runtime
        .load_session(AgentSessionLoad {
            agent_id: "codex".to_string(),
            task_id: "task-reuse-load".to_string(),
            session_id: "loaded-session".to_string(),
            cwd,
            model_id: None,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .expect("load session");

    assert_eq!(started.session_id, "counter-session-1");
    assert_eq!(loaded.session.session_id, "loaded-session");
    runtime
        .close_session(&started.session_id)
        .expect("close started session");
    runtime
        .close_session(&loaded.session.session_id)
        .expect("close loaded session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/load",
            "session/close",
            "session/close"
        ]
    );
}

#[test]
fn closed_agent_process_is_reused_for_later_session() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "__counter__") else {
        return;
    };
    let cwd = cwd_string();

    let first = runtime
        .start_session(start_request("task-reuse-before-close", cwd.clone()))
        .expect("first start");
    runtime
        .close_session(&first.session_id)
        .expect("close first session");
    let second = runtime
        .start_session(start_request("task-reuse-after-close", cwd))
        .expect("second start");
    runtime
        .close_session(&second.session_id)
        .expect("close second session");

    assert_eq!(first.session_id, "counter-session-1");
    assert_eq!(second.session_id, "counter-session-2");
    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/close",
            "session/new",
            "session/close"
        ]
    );
}

#[test]
fn load_session_registers_active_session_for_close() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "ignored-new-session") else {
        return;
    };

    let loaded = runtime
        .load_session(AgentSessionLoad {
            agent_id: "codex".to_string(),
            task_id: "task-load".to_string(),
            session_id: "loaded-session".to_string(),
            cwd: cwd_string(),
            model_id: None,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .expect("load session");

    assert_eq!(loaded.session.session_id, "loaded-session");
    assert!(loaded.replayed_messages.is_empty());
    runtime
        .close_session(&loaded.session.session_id)
        .expect("close loaded session");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/load", "session/close"]
    );
}

#[test]
fn delete_session_dispatches_to_active_session() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "delete-session") else {
        return;
    };

    let session = runtime
        .start_session(start_request("task-delete", cwd_string()))
        .expect("start session");
    runtime
        .delete_session(AgentSessionDelete {
            session_id: session.session_id,
        })
        .expect("delete session");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "session/delete"]
    );
}

#[cfg(unix)]
#[test]
fn active_session_start_timeout_reports_stable_error() {
    let mut manager = AcpActiveSessionManager::new(
        AgentRegistry::codex(AcpAgentConfig {
            agent_id: "codex".to_string(),
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "sleep 30".to_string()],
            env: Vec::new(),
            secret_env: Vec::new(),
        }),
        HostBridge::disabled(),
        AcpAuthMethodCache::default(),
    );
    manager.with_start_timeout(Duration::from_millis(20));

    let error = match manager.start_session(start_request("task-start-timeout", cwd_string())) {
        Ok(session) => panic!(
            "start unexpectedly succeeded after timeout: {}",
            session.session_id
        ),
        Err(error) => error.to_string(),
    };

    assert_eq!(error, "runtime not ready: ACP session start timed out");
}

#[test]
fn shutdown_closes_active_sessions() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "shutdown-session") else {
        return;
    };

    runtime
        .start_session(start_request("task-shutdown", cwd_string()))
        .expect("start session");
    runtime.shutdown().expect("shutdown runtime");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "session/close"]
    );
}

#[cfg(unix)]
#[test]
fn start_failure_reports_agent_error_instead_of_closed_start_channel() {
    let runtime = AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "exit 7".to_string()],
        env: Vec::new(),
        secret_env: Vec::new(),
    });

    let error = match runtime.start_session(start_request("task-start-failure", cwd_string())) {
        Ok(session) => panic!(
            "start unexpectedly succeeded after failed agent launch: {}",
            session.session_id
        ),
        Err(error) => error.to_string(),
    };

    assert!(
        !error.contains("channel is empty and sending half is closed"),
        "{error}"
    );
    assert!(error.contains("ACP error"), "{error}");
}
