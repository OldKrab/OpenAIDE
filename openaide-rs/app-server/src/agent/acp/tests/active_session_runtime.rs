use super::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{mpsc, Arc, Barrier};
use std::time::{Duration, Instant};

use crate::agent::acp_active_session_manager::AcpActiveSessionManager;
use crate::agent::acp_auth_method_cache::AcpAuthMethodCache;
use crate::agent::registry::{AgentCatalogRecord, AgentRegistry};
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::AgentPromptOutcome;
use crate::agent::{AgentSecretResolver, AgentSessionSetConfigOptionRequest, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::AgentAuthenticateStatus;

fn config_id(value: &str) -> crate::protocol::model::ConfigOptionCurrentValue {
    crate::protocol::model::ConfigOptionCurrentValue::id(value)
}

fn catalog_id<'a>(catalog: &'a ConfigOptionsCatalog, id: &str) -> Option<&'a str> {
    catalog
        .options
        .iter()
        .find(|option| option.id == id)
        .and_then(|option| option.current_value.as_id())
}

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

fn colliding_session_runtime(
    temp: &tempfile::TempDir,
    session_id: &str,
) -> Option<(AcpAgentRuntime, PathBuf, PathBuf)> {
    if !python3_available() {
        eprintln!("skipping ACP colliding-session fixture: python3 not found");
        return None;
    }

    let script_path = temp.path().join("fixture_agent.py");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    let records = ["agent-a", "agent-b"].map(|agent_id| {
        let log_path = temp.path().join(format!("{agent_id}.log"));
        AgentCatalogRecord::custom(
            agent_id.to_string(),
            agent_id.to_string(),
            "C".to_string(),
            true,
            "python3".to_string(),
            String::new(),
            vec![script_path.to_string_lossy().to_string()],
            HashMap::from([
                (
                    "OPENAIDE_ACP_FIXTURE_LOG".to_string(),
                    log_path.to_string_lossy().to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_SESSION".to_string(),
                    session_id.to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_CONFIG_OPTIONS".to_string(),
                    "1".to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                    "wait_for_cancel".to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_TITLE".to_string(),
                    format!("{agent_id} title"),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_LOG_DETAILS".to_string(),
                    "1".to_string(),
                ),
            ]),
            Vec::new(),
        )
    });
    let registry = AgentRegistry::from_agent_catalog(records.into_iter().collect())
        .expect("two-Agent fixture registry");
    Some((
        AcpAgentRuntime::new_with_registry(
            AgentRegistryHandle::new(registry),
            HostBridge::disabled(),
        ),
        temp.path().join("agent-a.log"),
        temp.path().join("agent-b.log"),
    ))
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
        context: Vec::new(),
        cancellation: TurnCancellation::new(),
        secret_resolver: None,
    }
}

#[test]
fn new_session_uses_agent_defaults_without_stored_config_reconciliation() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, _log_path)) = fixture_runtime(&temp, "reconciled-session") else {
        return;
    };
    let session = runtime
        .start_session(start_request("task-default-config", cwd_string()))
        .expect("start with Agent defaults");

    assert_eq!(session.session_id, "reconciled-session");
    assert!(session
        .config_catalog
        .is_some_and(|catalog| catalog.options.is_empty()));
}

#[test]
fn new_session_keeps_the_complete_agent_catalog() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        return;
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
                "catalog-session".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_CONFIG_OPTIONS".to_string(),
                "1".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    });
    let session = runtime
        .start_session(start_request("task-agent-catalog", cwd_string()))
        .expect("start with complete Agent catalog");
    let catalog = session.config_catalog.expect("config catalog");
    assert_eq!(catalog.options[0].id, "mode");
    assert_eq!(catalog.options[0].current_value.as_id(), Some("agent"));
    assert_eq!(catalog.options[1].id, "model");
    assert_eq!(
        catalog.options[1].current_value.as_id(),
        Some("gpt-5.6-sol")
    );
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

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for fixture condition");
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
import time

log_path = os.environ["OPENAIDE_ACP_FIXTURE_LOG"]
session_id = os.environ.get("OPENAIDE_ACP_FIXTURE_SESSION", "fixture-session")
prompt_mode = os.environ.get("OPENAIDE_ACP_FIXTURE_PROMPT_MODE", "")
supports_resume = os.environ.get("OPENAIDE_ACP_FIXTURE_RESUME", "1") == "1"
supports_close = os.environ.get("OPENAIDE_ACP_FIXTURE_CLOSE", "1") == "1"
with_config_options = os.environ.get("OPENAIDE_ACP_FIXTURE_CONFIG_OPTIONS", "") == "1"
config_response_delay = float(os.environ.get("OPENAIDE_ACP_FIXTURE_CONFIG_RESPONSE_DELAY", "0"))
fixture_title = os.environ.get("OPENAIDE_ACP_FIXTURE_TITLE", "")
log_details = os.environ.get("OPENAIDE_ACP_FIXTURE_LOG_DETAILS", "") == "1"
auth_method = os.environ.get("OPENAIDE_ACP_FIXTURE_AUTH_METHOD", "agent")
pending_prompt_ids = []
prompt_request_count = 0
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
    session_capabilities = {
        "delete": {},
        "list": {},
    }
    if supports_close:
        session_capabilities["close"] = {}
    if supports_resume:
        session_capabilities["resume"] = {}
    advertised_auth = {"id": "test-auth", "name": "Test auth"}
    if auth_method == "terminal":
        advertised_auth = {
            "id": "test-auth",
            "name": "Terminal auth",
            "type": "terminal",
            "args": ["login"],
            "env": {"LOGIN_MODE": "1"},
        }
    return {
        "protocolVersion": 1,
        "agentCapabilities": {
            "loadSession": True,
            "sessionCapabilities": session_capabilities,
        },
        "authMethods": [advertised_auth],
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
        elif session_id == "__second_new_error__" and next_session_number >= 2:
            sys.stdout.write(json.dumps({
                "jsonrpc": "2.0",
                "id": message.get("id"),
                "error": {"code": -32000, "message": "second session rejected"},
            }) + "\n")
            sys.stdout.flush()
        else:
            result = {"sessionId": session_id}
            if with_config_options:
                result["configOptions"] = [{
                    "id": "mode",
                    "name": "Mode",
                    "type": "select",
                    "currentValue": "agent",
                    "options": [
                        {"value": "agent", "name": "Agent"},
                        {"value": "agent-full-access", "name": "Agent (full access)"},
                    ],
                }, {
                    "id": "model",
                    "name": "Model",
                    "type": "select",
                    "currentValue": "gpt-5.6-sol",
                    "options": [
                        {"value": "gpt-5.6-sol", "name": "GPT-5.6-Sol"},
                        {"value": "gpt-5.5", "name": "GPT-5.5"},
                    ],
                }]
            respond(message, result)
            if prompt_mode == "title_after_new":
                notify_title("Agent generated title")
            elif fixture_title:
                notify_title(fixture_title)
    elif method == "session/load":
        if prompt_mode == "load_replay":
            notify_text_chunk("replayed through active worker")
        respond(message, {"configOptions": []})
    elif method == "session/resume":
        respond(message, {
            "configOptions": [{
                "id": "model",
                "name": "Model",
                "type": "select",
                "currentValue": "gpt-5.6-sol",
                "options": [
                    {"value": "gpt-5.6-sol", "name": "GPT-5.6-Sol"},
                    {"value": "gpt-5.5", "name": "GPT-5.5"},
                ],
            }],
        })
        if session_id == "idle-session":
            notify_title("Title after idle resume")
    elif method == "session/list":
        respond(message, {"sessions": []})
    elif method == "authenticate":
        respond(message, {})
    elif method == "session/prompt":
        if log_details:
            log("prompt:" + json.dumps(message.get("params", {}), sort_keys=True))
        prompt_request_count += 1
        if prompt_mode == "host_terminal_wait_for_cancel":
            pending_prompt_ids.append(message.get("id"))
            request_terminal("prompt-terminal-create-1")
            request_terminal("prompt-terminal-create-2")
        elif prompt_mode == "wait_for_cancel" or (
            prompt_mode == "delay_first_cancel_response" and prompt_request_count == 1
        ):
            pending_prompt_ids.append(message.get("id"))
        elif prompt_mode == "late_text_after_response":
            respond(message, {"stopReason": "end_turn"})
            notify_text_chunk("late response text")
        elif prompt_mode == "late_text_after_prompt_return":
            respond(message, {"stopReason": "end_turn"})
            time.sleep(0.25)
            notify_text_chunk("session-owned late text")
        elif prompt_mode == "title_during_prompt":
            notify_title("Title from active turn")
            respond(message, {"stopReason": "end_turn"})
        elif prompt_mode.startswith("stop_reason:"):
            respond(message, {"stopReason": prompt_mode.split(":", 1)[1]})
        else:
            respond(message, {"stopReason": "end_turn"})
    elif method == "session/set_config_option":
        params = message.get("params", {})
        if log_details:
            log("config:" + json.dumps(params, sort_keys=True))
        if config_response_delay > 0:
            time.sleep(config_response_delay)
        config_id = params.get("configId", "model")
        value = params.get("value", "gpt-5")
        if prompt_mode == "config_update_before_config_response":
            notify("session/update", {
                "sessionId": session_id,
                "update": {
                    "sessionUpdate": "config_option_update",
                    "configOptions": [{
                        "id": "model",
                        "name": "Model",
                        "type": "select",
                        "currentValue": "gpt-intermediate",
                        "options": [
                            {"value": "gpt-intermediate", "name": "Intermediate"},
                            {"value": "gpt-final", "name": "Final"},
                        ],
                    }],
                },
            })
        if with_config_options:
            respond(message, {
                "configOptions": [{
                    "id": "mode", "name": "Mode", "type": "select",
                    "currentValue": "agent",
                    "options": [
                        {"value": "agent", "name": "Agent"},
                        {"value": "agent-full-access", "name": "Agent (full access)"},
                    ],
                }, {
                    "id": "model", "name": "Model", "type": "select",
                    "currentValue": value,
                    "options": [
                        {"value": "gpt-5.6-sol", "name": "GPT-5.6-Sol"},
                        {"value": "gpt-5.5", "name": "GPT-5.5"},
                    ],
                }],
            })
            continue
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
        if prompt_mode == "delay_first_cancel_response":
            time.sleep(0.3)
        while pending_prompt_ids:
            respond_id(pending_prompt_ids.pop(0), {"stopReason": "cancelled"})
    elif method == "session/close":
        respond(message, {})
        if session_id == "idle-session":
            closed_session_count += 1
            if closed_session_count >= 2:
                break
        elif session_id == "__counter__":
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
                agent_id: "codex".to_string(),
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
        .close_session(&session.key())
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
fn new_native_session_waits_for_history_before_idle_close() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "idle-session") else {
        return;
    };
    let runtime = runtime.with_session_idle_timeout(Duration::from_millis(50));

    let session = runtime
        .start_session(start_request("task-idle-session", cwd_string()))
        .expect("start session");
    let session_sink = Arc::new(CapturingSessionSink::default());
    runtime
        .attach_session_event_sink(&session.key(), session_sink.clone())
        .expect("attach session sink");

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new"]
    );

    assert_eq!(
        runtime
            .prompt(
                AgentPrompt {
                    agent_id: "codex".to_string(),
                    task_id: "task-idle-session".to_string(),
                    session_id: session.session_id,
                    text: "create durable history".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
            .expect("first prompt remains usable after the empty-session idle timeout"),
        AgentPromptOutcome::EndTurn
    );
    wait_for_method(&log_path, "session/close");
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
fn resumed_native_session_is_closed_after_the_idle_timeout() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "idle-session") else {
        return;
    };
    let runtime = runtime.with_session_idle_timeout(Duration::from_millis(50));

    let resumed = runtime
        .resume_session(AgentSessionResume {
            agent_id: "codex".to_string(),
            task_id: "task-idle-session".to_string(),
            session_id: "idle-session".to_string(),
            cwd: cwd_string(),
            model_id: None,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .expect("resume expired session");
    let session_sink = Arc::new(CapturingSessionSink::default());
    runtime
        .attach_session_event_sink(&resumed.key(), session_sink.clone())
        .expect("attach resumed session sink");
    wait_until(|| !session_sink.metadata_updates().is_empty());
    assert_eq!(
        session_sink.metadata_updates(),
        vec![AgentSessionMetadataUpdate {
            title: AgentMetadataField::Value("Title after idle resume".to_string()),
            updated_at: AgentMetadataField::Unchanged,
        }]
    );
    wait_for_method(&log_path, "session/close");
    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/resume", "session/close"]
    );
}

#[test]
fn active_prompt_suspends_native_session_idle_expiration() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        return;
    }
    let script_path = temp.path().join("fixture_agent.py");
    let log_path = temp.path().join("fixture.log");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    let runtime = Arc::new(
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
                    "idle-running-prompt".to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                    "wait_for_cancel".to_string(),
                ),
            ],
            secret_env: Vec::new(),
        })
        .with_session_idle_timeout(Duration::from_millis(50)),
    );
    let session = runtime
        .start_session(start_request("task-idle-running", cwd_string()))
        .expect("start session");
    let prompt_session_id = session.session_id.clone();
    let prompt_handle = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    agent_id: "codex".to_string(),
                    task_id: "task-idle-running".to_string(),
                    session_id: prompt_session_id,
                    text: "keep working".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });
    wait_for_method(&log_path, "session/prompt");

    std::thread::sleep(Duration::from_millis(100));
    assert!(!read_fixture_methods(&log_path)
        .iter()
        .any(|method| method == "session/close"));

    runtime
        .cancel_session(&session.key())
        .expect("cancel prompt");
    assert_eq!(
        prompt_handle.join().expect("prompt thread").unwrap(),
        AgentPromptOutcome::Cancelled
    );
    wait_for_method(&log_path, "session/close");
}

#[test]
fn idle_timeout_does_not_close_when_the_agent_lacks_close_capability() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        return;
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
                "idle-without-close".to_string(),
            ),
            ("OPENAIDE_ACP_FIXTURE_CLOSE".to_string(), "0".to_string()),
        ],
        secret_env: Vec::new(),
    })
    .with_session_idle_timeout(Duration::from_millis(50));
    let session = runtime
        .start_session(start_request("task-idle-without-close", cwd_string()))
        .expect("start session");

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new"]
    );
    runtime
        .close_session(&session.key())
        .expect("release local session worker");
    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new"]
    );
}

#[test]
fn listing_then_starting_reuses_one_agent_process() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "list-before-start-session") else {
        return;
    };

    runtime
        .list_sessions(AgentListSessionsRequest {
            agent_id: "codex".to_string(),
            cwd: cwd_string(),
            cursor: None,
        })
        .expect("list sessions");
    let session = runtime
        .start_session(start_request("task-after-list", cwd_string()))
        .expect("start session");
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/list", "session/new", "session/close"]
    );
}

#[test]
fn task_trace_preserves_initialize_capabilities_after_process_warmup() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, _log_path)) = fixture_runtime(&temp, "trace-after-warmup-session") else {
        return;
    };
    let trace_state = crate::agent::acp_trace::AcpTraceState::disabled(temp.path());
    trace_state.set_enabled(true).expect("enable ACP trace");
    let runtime = runtime.with_trace_state(trace_state);

    runtime
        .list_sessions(AgentListSessionsRequest {
            agent_id: "codex".to_string(),
            cwd: cwd_string(),
            cursor: None,
        })
        .expect("warm shared Agent process");
    let session = runtime
        .start_session(start_request("task-trace-after-warmup", cwd_string()))
        .expect("start traced session");
    runtime
        .close_session(&session.key())
        .expect("close session");

    let trace_dir = temp.path().join("diagnostics").join("acp-traces");
    let trace_path = fs::read_dir(trace_dir)
        .expect("trace directory")
        .next()
        .expect("task trace")
        .expect("trace entry")
        .path();
    let trace = fs::read_to_string(trace_path).expect("trace content");
    assert!(trace.contains("\"event\":\"initialize.snapshot\""));
    assert!(trace.contains("\"sessionId\":\"trace-after-warmup-session\""));
    assert!(trace.contains("\"elicitation\":{\"form\":{}}"));
    assert!(trace.contains("\"source\":\"shared_process\""));
}

#[test]
fn listing_sessions_reuses_the_active_agent_process() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "shared-list-session") else {
        return;
    };
    let session = runtime
        .start_session(start_request("task-shared-list", cwd_string()))
        .expect("start session");

    runtime
        .list_sessions(AgentListSessionsRequest {
            agent_id: "codex".to_string(),
            cwd: cwd_string(),
            cursor: None,
        })
        .expect("list sessions");
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "session/list", "session/close"]
    );
}

#[test]
fn probing_reuses_the_active_agent_process() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "shared-probe-session") else {
        return;
    };
    let session = runtime
        .start_session(start_request("task-shared-probe", cwd_string()))
        .expect("start session");

    runtime
        .probe(crate::agent::AgentProbeRequest {
            agent_id: "codex".to_string(),
        })
        .expect("probe agent");
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "session/close"]
    );
}

#[test]
fn authentication_reuses_the_active_agent_process() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "shared-auth-session") else {
        return;
    };
    runtime
        .start_session(start_request("task-shared-auth", cwd_string()))
        .expect("start session");

    runtime
        .authenticate(crate::agent::AgentAuthenticateRequest {
            agent_id: "codex".to_string(),
            method_id: "test-auth".to_string(),
            env: HashMap::new(),
            secret_env: Vec::new(),
            secret_storage_agent_id: None,
            terminal_confirmed: false,
        })
        .expect("authenticate agent");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "authenticate"]
    );

    // Closing is covered by the session lifecycle tests. Keep teardown from
    // turning close latency into a failure of this process-reuse contract.
    runtime.shutdown().expect("shut down runtime");
}

#[test]
fn env_var_authentication_relaunches_with_secure_host_values() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        return;
    }
    let script_path = temp.path().join("fixture_agent.py");
    let log_path = temp.path().join("fixture.log");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    let (host_bridge, requests) = HostBridge::channel_with_timeout(Duration::from_secs(2));
    let response_bridge = host_bridge.clone();
    let host = std::thread::spawn(move || {
        let request = requests
            .recv_timeout(Duration::from_secs(2))
            .expect("secret host request");
        assert_eq!(request.method, "agent/secret_env");
        let params = request.params.as_ref().expect("secret params");
        assert_eq!(params["agent_id"], "codex.auth.6170692d6b6579");
        assert_eq!(params["names"], serde_json::json!(["OPENAIDE_SECRET_TEST"]));
        assert!(response_bridge.try_handle_response(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": { "env": { "OPENAIDE_SECRET_TEST": "secure-token" } },
        })));
    });
    let runtime = AcpAgentRuntime::new_with_host(
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
                    "env-auth".to_string(),
                ),
            ],
            secret_env: Vec::new(),
        },
        host_bridge,
    );

    runtime
        .authenticate(crate::agent::AgentAuthenticateRequest {
            agent_id: "codex".to_string(),
            method_id: "test-auth".to_string(),
            env: HashMap::new(),
            secret_env: vec!["OPENAIDE_SECRET_TEST".to_string()],
            secret_storage_agent_id: Some("codex.auth.6170692d6b6579".to_string()),
            terminal_confirmed: false,
        })
        .expect("authenticate with env var");
    host.join().expect("secret host");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["secret:secure-token", "initialize", "authenticate"]
    );
}

#[test]
fn terminal_authentication_waits_for_user_confirmation_before_acp_authenticate() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        return;
    }
    let script_path = temp.path().join("fixture_agent.py");
    let log_path = temp.path().join("fixture.log");
    fs::write(&script_path, fixture_agent_script()).expect("fixture agent script");
    let (host_bridge, requests) = HostBridge::channel_with_timeout(Duration::from_secs(2));
    let response_bridge = host_bridge.clone();
    let host = std::thread::spawn(move || {
        let request = requests
            .recv_timeout(Duration::from_secs(2))
            .expect("visible terminal request");
        assert_eq!(request.method, "agent/auth_terminal");
        let params = request.params.as_ref().expect("terminal params");
        assert_eq!(params["command"], "python3");
        assert_eq!(params["args"].as_array().unwrap().last().unwrap(), "login");
        assert_eq!(params["env"]["LOGIN_MODE"], "1");
        assert!(response_bridge.try_handle_response(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": {},
        })));
    });
    let runtime = AcpAgentRuntime::new_with_host(
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
                    "terminal-auth".to_string(),
                ),
                (
                    "OPENAIDE_ACP_FIXTURE_AUTH_METHOD".to_string(),
                    "terminal".to_string(),
                ),
            ],
            secret_env: Vec::new(),
        },
        host_bridge,
    );

    let awaiting = runtime
        .authenticate(crate::agent::AgentAuthenticateRequest {
            agent_id: "codex".to_string(),
            method_id: "test-auth".to_string(),
            env: HashMap::new(),
            secret_env: Vec::new(),
            secret_storage_agent_id: None,
            terminal_confirmed: false,
        })
        .expect("open terminal auth");
    assert!(matches!(
        awaiting.status,
        AgentAuthenticateStatus::AwaitingUser
    ));
    assert_eq!(read_fixture_methods(&log_path), ["initialize"]);
    host.join().expect("visible terminal host");

    runtime
        .authenticate(crate::agent::AgentAuthenticateRequest {
            agent_id: "codex".to_string(),
            method_id: "test-auth".to_string(),
            env: HashMap::new(),
            secret_env: Vec::new(),
            secret_storage_agent_id: None,
            terminal_confirmed: true,
        })
        .expect("confirm terminal auth");
    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "authenticate"]
    );
}

#[test]
fn session_sink_receives_text_update_after_prompt_has_returned() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "session-owned-late-text") else {
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
                "session-owned-late-text".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                "late_text_after_prompt_return".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    });
    let prompt_sink = Arc::new(CapturingEventSink::default());
    let session_sink = Arc::new(CapturingSessionSink::default());
    let session = runtime
        .start_session(start_request("task-session-owned-text", cwd_string()))
        .expect("start session");
    runtime
        .attach_session_event_sink(&session.key(), session_sink.clone())
        .expect("attach session sink");

    runtime
        .prompt(
            AgentPrompt {
                agent_id: "codex".to_string(),
                task_id: "task-session-owned-text".to_string(),
                session_id: session.session_id.clone(),
                text: "hello".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            prompt_sink.clone(),
        )
        .expect("prompt");
    assert!(prompt_sink.events().is_empty());
    let deadline = Instant::now() + Duration::from_secs(2);
    while session_sink.events().is_empty() {
        assert!(Instant::now() < deadline, "late session update timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert!(matches!(
        session_sink.events().as_slice(),
        [AgentEvent::MessageChunk {
            role: AgentMessageRole::Agent,
            part: AgentMessagePart::Text { text },
            ..
        }] if text == "session-owned late text"
    ));
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
    runtime.close_session(&session.key()).unwrap();
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
            secret_resolver: None,
        })
        .expect("resume active session");
    assert_eq!(resumed.session_id, session.session_id);

    runtime
        .attach_session_event_sink(&session.key(), Arc::new(CapturingSessionSink::default()))
        .expect("attach session sink");
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "session/close"]
    );
}

#[test]
fn resume_after_runtime_restart_dispatches_to_the_agent() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime(&temp, "resumed-session") else {
        return;
    };

    let resumed = runtime
        .resume_session(AgentSessionResume {
            agent_id: "codex".to_string(),
            task_id: "task-resume-after-restart".to_string(),
            session_id: "resumed-session".to_string(),
            cwd: cwd_string(),
            model_id: None,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .expect("resume inactive session");

    assert_eq!(resumed.session_id, "resumed-session");
    assert_eq!(
        resumed
            .config_catalog
            .as_ref()
            .and_then(|catalog| catalog.options.first())
            .and_then(|option| option.current_value.as_id()),
        Some("gpt-5.6-sol")
    );
    runtime
        .close_session(&resumed.key())
        .expect("close session");
    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/resume", "session/close"]
    );
}

#[test]
fn resume_without_agent_capability_returns_typed_capability_error() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        return;
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
                "unsupported-resume-session".to_string(),
            ),
            ("OPENAIDE_ACP_FIXTURE_RESUME".to_string(), "0".to_string()),
        ],
        secret_env: Vec::new(),
    });

    let error = match runtime.resume_session(AgentSessionResume {
        agent_id: "codex".to_string(),
        task_id: "task-unsupported-resume".to_string(),
        session_id: "unsupported-resume-session".to_string(),
        cwd: cwd_string(),
        model_id: None,
        cancellation: TurnCancellation::new(),
        secret_resolver: None,
    }) {
        Ok(session) => panic!("unexpectedly resumed {}", session.session_id),
        Err(error) => error,
    };

    assert!(matches!(error, RuntimeError::CapabilityMissing(_)));
    assert_eq!(read_fixture_methods(&log_path), ["initialize"]);
}

#[test]
fn config_response_and_prior_updates_reach_the_bound_task_before_the_caller() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, _log_path)) = fixture_runtime_with_prompt_mode(
        &temp,
        "ordered-config-session",
        "config_update_before_config_response",
    ) else {
        return;
    };
    let runtime = Arc::new(runtime);
    let session = runtime
        .start_session(start_request("task-ordered-config", cwd_string()))
        .expect("start session");
    let (update_started_tx, update_started_rx) = mpsc::channel();
    let release_update = Arc::new(Barrier::new(2));
    runtime
        .attach_session_event_sink(
            &session.key(),
            Arc::new(BlockingConfigSessionSink {
                update_started_tx,
                release_update: release_update.clone(),
                received: AtomicUsize::new(0),
            }),
        )
        .expect("attach session sink");

    let (result_tx, result_rx) = mpsc::channel();
    let runtime_for_change = runtime.clone();
    let session_id = session.session_id.clone();
    let change = std::thread::spawn(move || {
        let result =
            runtime_for_change.set_session_config_option(AgentSessionSetConfigOptionRequest {
                agent_id: "codex".to_string(),
                session_id,
                config_id: "model".to_string(),
                value: config_id("gpt-final"),
            });
        result_tx.send(result).expect("report config result");
    });

    let prior_catalog = update_started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("prior config update should reach the bound session sink");
    let early_result = match result_rx.try_recv() {
        Ok(result) => Some(result),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => panic!("config result channel disconnected"),
    };
    let response_overtook_update = early_result.is_some();
    release_update.wait();
    let response_catalog = update_started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the response catalog should reach the bound session sink");
    let result = early_result.unwrap_or_else(|| {
        result_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("config response should follow the prior update")
    });
    change.join().expect("config change thread");
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert_eq!(
        catalog_id(&prior_catalog, "model"),
        Some("gpt-intermediate")
    );
    assert!(
        !response_overtook_update,
        "the Agent response must not overtake a preceding session update"
    );
    assert_eq!(catalog_id(&response_catalog, "model"), Some("gpt-final"));
    assert_eq!(
        catalog_id(&result.expect("set config option"), "model"),
        Some("gpt-final")
    );
}

struct BlockingConfigSessionSink {
    update_started_tx: mpsc::Sender<ConfigOptionsCatalog>,
    release_update: Arc<Barrier>,
    received: AtomicUsize,
}

impl AgentSessionEventSink for BlockingConfigSessionSink {
    fn config_options_changed(&self, catalog: ConfigOptionsCatalog) -> Result<(), RuntimeError> {
        self.update_started_tx
            .send(catalog)
            .map_err(|error| RuntimeError::Internal(error.to_string()))?;
        if self.received.fetch_add(1, Ordering::SeqCst) == 0 {
            self.release_update.wait();
        }
        Ok(())
    }

    fn commands_changed(&self, _catalog: AgentCommandsCatalog) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[test]
fn different_agents_may_own_the_same_native_session_id() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, agent_a_log, agent_b_log)) =
        colliding_session_runtime(&temp, "shared-native-session")
    else {
        return;
    };
    let runtime = Arc::new(runtime);
    let mut agent_a_request = start_request("task-agent-a", cwd_string());
    agent_a_request.agent_id = "agent-a".to_string();
    let agent_a = runtime
        .start_session(agent_a_request)
        .expect("start Agent A session");
    let mut agent_b_request = start_request("task-agent-b", cwd_string());
    agent_b_request.agent_id = "agent-b".to_string();

    let agent_b = match runtime.start_session(agent_b_request) {
        Ok(session) => session,
        Err(error) => {
            let _ = runtime.shutdown();
            panic!("Agent B must not collide with Agent A's native session id: {error}");
        }
    };

    assert_eq!(agent_a.session_id, "shared-native-session");
    assert_eq!(agent_b.session_id, "shared-native-session");
    assert_eq!(agent_a.agent_id, "agent-a");
    assert_eq!(agent_b.agent_id, "agent-b");

    for (agent_id, task_id) in [("agent-a", "task-agent-a"), ("agent-b", "task-agent-b")] {
        let resumed = runtime
            .resume_session(AgentSessionResume {
                agent_id: agent_id.to_string(),
                task_id: task_id.to_string(),
                session_id: "shared-native-session".to_string(),
                cwd: cwd_string(),
                model_id: None,
                cancellation: TurnCancellation::new(),
                secret_resolver: None,
            })
            .expect("resume the matching Agent session");
        assert_eq!(resumed.agent_id, agent_id);
    }

    let agent_a_sink = Arc::new(CapturingSessionSink::default());
    let agent_b_sink = Arc::new(CapturingSessionSink::default());
    runtime
        .attach_session_event_sink(&agent_a.key(), agent_a_sink.clone())
        .expect("attach Agent A session sink");
    runtime
        .attach_session_event_sink(&agent_b.key(), agent_b_sink.clone())
        .expect("attach Agent B session sink");
    let metadata_deadline = Instant::now() + Duration::from_secs(2);
    while (agent_a_sink.metadata_updates().is_empty() || agent_b_sink.metadata_updates().is_empty())
        && Instant::now() < metadata_deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        agent_a_sink.metadata_updates()[0].title,
        AgentMetadataField::Value("agent-a title".to_string())
    );
    assert_eq!(
        agent_b_sink.metadata_updates()[0].title,
        AgentMetadataField::Value("agent-b title".to_string())
    );

    let agent_a_attachment_path = temp.path().join("agent-a.txt");
    let agent_b_attachment_path = temp.path().join("agent-b.txt");
    fs::write(&agent_a_attachment_path, "Agent A context").expect("Agent A attachment");
    fs::write(&agent_b_attachment_path, "Agent B context").expect("Agent B attachment");
    let (agent_a_prompt_tx, agent_a_prompt_rx) = mpsc::channel();
    let agent_a_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        let session_id = agent_a.session_id.clone();
        move || {
            let result = runtime.prompt(
                AgentPrompt {
                    agent_id: "agent-a".to_string(),
                    task_id: "task-agent-a".to_string(),
                    session_id,
                    text: "continue Agent A".to_string(),
                    attachments: vec![Attachment {
                        kind: "file".to_string(),
                        label: "agent-a.txt".to_string(),
                        path: Some(agent_a_attachment_path.to_string_lossy().to_string()),
                        payload: None,
                    }],
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            );
            let _ = agent_a_prompt_tx.send(result);
        }
    });
    let (agent_b_prompt_tx, agent_b_prompt_rx) = mpsc::channel();
    let agent_b_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        let session_id = agent_b.session_id.clone();
        move || {
            let result = runtime.prompt(
                AgentPrompt {
                    agent_id: "agent-b".to_string(),
                    task_id: "task-agent-b".to_string(),
                    session_id,
                    text: "continue Agent B".to_string(),
                    attachments: vec![Attachment {
                        kind: "file".to_string(),
                        label: "agent-b.txt".to_string(),
                        path: Some(agent_b_attachment_path.to_string_lossy().to_string()),
                        payload: None,
                    }],
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            );
            let _ = agent_b_prompt_tx.send(result);
        }
    });
    wait_for_method(&agent_a_log, "session/prompt");
    wait_for_method(&agent_b_log, "session/prompt");

    let agent_a_catalog = runtime
        .set_session_config_option(AgentSessionSetConfigOptionRequest {
            agent_id: "agent-a".to_string(),
            session_id: agent_a.session_id.clone(),
            config_id: "model".to_string(),
            value: config_id("gpt-5.5"),
        })
        .expect("set Agent A config");
    let agent_b_catalog = runtime
        .set_session_config_option(AgentSessionSetConfigOptionRequest {
            agent_id: "agent-b".to_string(),
            session_id: agent_b.session_id.clone(),
            config_id: "model".to_string(),
            value: config_id("gpt-5.6-sol"),
        })
        .expect("set Agent B config");
    assert_eq!(catalog_id(&agent_a_catalog, "model"), Some("gpt-5.5"));
    assert_eq!(catalog_id(&agent_b_catalog, "model"), Some("gpt-5.6-sol"));

    runtime
        .cancel_session(&agent_a.key())
        .expect("cancel Agent A prompt");
    agent_a_prompt_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("Agent A prompt returned")
        .expect("Agent A prompt cancelled cleanly");
    assert!(matches!(
        agent_b_prompt_rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    runtime
        .cancel_session(&agent_b.key())
        .expect("cancel Agent B prompt");
    agent_b_prompt_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("Agent B prompt returned")
        .expect("Agent B prompt cancelled cleanly");
    agent_a_prompt.join().expect("Agent A prompt thread");
    agent_b_prompt.join().expect("Agent B prompt thread");

    runtime
        .close_session(&agent_a.key())
        .expect("close Agent A session");
    let agent_a_resume = runtime.resume_session(AgentSessionResume {
        agent_id: "agent-a".to_string(),
        task_id: "task-agent-a".to_string(),
        session_id: agent_a.session_id.clone(),
        cwd: cwd_string(),
        model_id: None,
        cancellation: TurnCancellation::new(),
        secret_resolver: None,
    });
    assert!(agent_a_resume.is_err());
    runtime
        .resume_session(AgentSessionResume {
            agent_id: "agent-b".to_string(),
            task_id: "task-agent-b".to_string(),
            session_id: agent_b.session_id.clone(),
            cwd: cwd_string(),
            model_id: None,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .expect("Agent B remains active after Agent A closes");
    runtime
        .close_session(&agent_b.key())
        .expect("close Agent B session");

    let agent_a_methods = read_fixture_methods(&agent_a_log);
    let agent_b_methods = read_fixture_methods(&agent_b_log);
    for (methods, own_attachment, other_attachment, own_model) in [
        (&agent_a_methods, "agent-a.txt", "agent-b.txt", "gpt-5.5"),
        (
            &agent_b_methods,
            "agent-b.txt",
            "agent-a.txt",
            "gpt-5.6-sol",
        ),
    ] {
        assert!(methods.iter().any(|line| line == "session/prompt"));
        assert!(methods
            .iter()
            .any(|line| line.starts_with("prompt:") && line.contains(own_attachment)));
        assert!(!methods.iter().any(|line| line.contains(other_attachment)));
        assert!(methods
            .iter()
            .any(|line| { line.starts_with("config:") && line.contains(own_model) }));
        assert!(methods.iter().any(|line| line == "session/cancel"));
        assert!(methods.iter().any(|line| line == "session/close"));
    }
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
        .attach_session_event_sink(&session.key(), sink.clone())
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
    runtime.close_session(&session.key()).unwrap();
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
        .attach_session_event_sink(&session.key(), sink.clone())
        .expect("attach session sink");

    runtime
        .prompt(
            AgentPrompt {
                agent_id: "codex".to_string(),
                task_id: "task-active-metadata".to_string(),
                session_id: session.session_id.clone(),
                text: "do work".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(CapturingEventSink::default()),
        )
        .expect("prompt");

    let started = Instant::now();
    while sink.metadata_updates().is_empty() && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        sink.metadata_updates(),
        vec![AgentSessionMetadataUpdate {
            title: AgentMetadataField::Value("Title from active turn".to_string()),
            updated_at: AgentMetadataField::Unchanged,
        }]
    );
    runtime.close_session(&session.key()).unwrap();
}

#[test]
fn inactive_session_registry_reports_stable_binding_errors() {
    let runtime = inactive_runtime();
    let missing_session = AgentSessionKey::new("codex", "missing-session");

    let attach_error = runtime
        .attach_session_event_sink(&missing_session, Arc::new(CapturingSessionSink::default()))
        .expect_err("missing attach should fail")
        .to_string();
    assert_eq!(attach_error, "runtime not ready: ACP session is not active");

    let prompt_error = runtime
        .prompt(
            AgentPrompt {
                agent_id: "codex".to_string(),
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
            agent_id: "codex".to_string(),
            session_id: "missing-session".to_string(),
        })
        .expect_err("missing delete should fail")
        .to_string();
    assert_eq!(delete_error, "runtime not ready: ACP session is not active");
}

#[test]
fn inactive_session_cancel_and_close_are_idempotent() {
    let runtime = inactive_runtime();
    let missing_session = AgentSessionKey::new("codex", "missing-session");

    runtime
        .cancel_session(&missing_session)
        .expect("missing cancel remains idempotent");
    runtime
        .close_session(&missing_session)
        .expect("missing close remains idempotent");
}

#[test]
fn cancel_session_dispatches_to_active_prompt() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "cancel-session") else {
        return;
    };
    let script_path = temp.path().join("fixture_agent.py");
    let trace_state = crate::agent::acp_trace::AcpTraceState::disabled(temp.path());
    trace_state.set_enabled(true).expect("enable ACP trace");
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
    })
    .with_trace_state(trace_state);

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
                    agent_id: "codex".to_string(),
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
        .cancel_session(&session.key())
        .expect("cancel session");
    let prompt_result = prompt_handle.join().expect("prompt thread");
    prompt_result.expect("prompt should finish after cancel");
    runtime
        .close_session(&session.key())
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

    let trace_dir = temp.path().join("diagnostics").join("acp-traces");
    let trace_path = fs::read_dir(trace_dir)
        .expect("trace directory")
        .next()
        .expect("task trace")
        .expect("trace entry")
        .path();
    let trace = fs::read_to_string(trace_path).expect("trace content");
    assert!(trace.contains("\"event\":\"session/cancel.worker_received\""));
    assert!(trace.contains("\"event\":\"session/cancel.dispatch_started\""));
    assert!(trace.contains("\"event\":\"session/cancel.dispatch_completed\""));
    assert!(trace.contains("\"event\":\"session/cancel.prompt_settled\""));
    assert!(trace.contains("\"elapsed_ms\":"));
}

#[test]
fn cancelled_prompt_settles_before_session_accepts_next_prompt() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) = fixture_runtime_with_prompt_mode(
        &temp,
        "cancel-then-send-session",
        "delay_first_cancel_response",
    ) else {
        return;
    };
    let runtime = Arc::new(runtime);
    let session = runtime
        .start_session(start_request("task-cancel-then-send", cwd_string()))
        .expect("start session");
    let first_cancellation = TurnCancellation::new();
    let prompt_cancellation = first_cancellation.clone();
    let prompt_session_id = session.session_id.clone();
    let first_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    agent_id: "codex".to_string(),
                    task_id: "task-cancel-then-send".to_string(),
                    session_id: prompt_session_id,
                    text: "cancel me".to_string(),
                    attachments: Vec::new(),
                    cancellation: prompt_cancellation,
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });

    wait_for_method(&log_path, "session/prompt");
    first_cancellation.cancel();
    runtime
        .cancel_session(&session.key())
        .expect("cancel first prompt");
    let second_session_id = session.session_id.clone();
    let second_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    agent_id: "codex".to_string(),
                    task_id: "task-cancel-then-send".to_string(),
                    session_id: second_session_id,
                    text: "continue after cancel".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });

    first_prompt
        .join()
        .expect("first prompt thread")
        .expect("cancelled prompt should settle cleanly");
    second_prompt
        .join()
        .expect("second prompt thread")
        .expect("next prompt should start after cancellation settles");
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/prompt",
            "session/cancel",
            "session/prompt",
            "session/close"
        ]
    );
}

#[test]
fn prompt_preserves_agent_stop_reason() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, _log_path)) =
        fixture_runtime_with_prompt_mode(&temp, "stop-reason-session", "stop_reason:max_tokens")
    else {
        return;
    };
    let session = runtime
        .start_session(start_request("task-stop-reason", cwd_string()))
        .expect("start session");

    let outcome = runtime
        .prompt(
            AgentPrompt {
                agent_id: "codex".to_string(),
                task_id: "task-stop-reason".to_string(),
                session_id: session.session_id.clone(),
                text: "continue".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(CapturingEventSink::default()),
        )
        .expect("prompt completes");

    assert_eq!(outcome, AgentPromptOutcome::MaxTokens);
    runtime
        .close_session(&session.key())
        .expect("close session");
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
                    agent_id: "codex".to_string(),
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
        .cancel_session(&session.key())
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
        .close_session(&session.key())
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
fn second_prompt_is_rejected_while_prior_prompt_is_running() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "sequential-session") else {
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
                "sequential-session".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                "wait_for_cancel".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    }));

    let session = runtime
        .start_session(start_request("task-sequential", cwd_string()))
        .expect("start session");
    let first_session_id = session.session_id.clone();
    let first_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    agent_id: "codex".to_string(),
                    task_id: "task-sequential".to_string(),
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

    let second_session_id = session.session_id.clone();
    let (second_result_tx, second_result_rx) = mpsc::channel();
    let second_prompt = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            let result = runtime.prompt(
                AgentPrompt {
                    agent_id: "codex".to_string(),
                    task_id: "task-sequential".to_string(),
                    session_id: second_session_id,
                    text: "second prompt".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            );
            let _ = second_result_tx.send(result);
        }
    });

    std::thread::sleep(Duration::from_millis(100));
    let prompt_method_count = read_fixture_methods(&log_path)
        .iter()
        .filter(|method| method.as_str() == "session/prompt")
        .count();
    let second_result = second_result_rx.recv_timeout(Duration::from_millis(250));
    runtime
        .cancel_session(&session.key())
        .expect("cancel session");
    first_prompt
        .join()
        .expect("first prompt thread")
        .expect("first prompt should finish after cancel");
    second_prompt.join().expect("second prompt thread");
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert_eq!(
        prompt_method_count, 1,
        "a second prompt must not reach an Agent with an active ACP prompt turn"
    );
    let error = second_result
        .expect("second prompt should be rejected before the active prompt finishes")
        .expect_err("second prompt should be rejected");
    assert!(error.to_string().contains("active prompt"));
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
fn steering_sends_an_additional_prompt_while_primary_prompt_is_running() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((_runtime, log_path)) = fixture_runtime(&temp, "steering-session") else {
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
                "steering-session".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_PROMPT_MODE".to_string(),
                "wait_for_cancel".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    }));
    let session = runtime
        .start_session(start_request("task-steering", cwd_string()))
        .expect("start session");
    let primary_session_id = session.session_id.clone();
    let primary = std::thread::spawn({
        let runtime = runtime.clone();
        move || {
            runtime.prompt(
                AgentPrompt {
                    agent_id: "codex".to_string(),
                    task_id: "task-steering".to_string(),
                    session_id: primary_session_id,
                    text: "start work".to_string(),
                    attachments: Vec::new(),
                    cancellation: TurnCancellation::new(),
                },
                Arc::new(CapturingEventSink::default()),
            )
        }
    });
    wait_for_method_count(&log_path, "session/prompt", 1);

    runtime
        .steer(AgentPrompt {
            agent_id: "codex".to_string(),
            task_id: "task-steering".to_string(),
            session_id: session.session_id.clone(),
            text: "also inspect tests".to_string(),
            attachments: Vec::new(),
            cancellation: TurnCancellation::new(),
        })
        .expect("steering should be admitted without waiting for its response");
    wait_for_method_count(&log_path, "session/prompt", 2);

    runtime
        .cancel_session(&session.key())
        .expect("cancel primary prompt");
    primary
        .join()
        .expect("primary prompt thread")
        .expect("primary prompt should settle after cancellation");
    runtime
        .close_session(&session.key())
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
                    agent_id: "codex".to_string(),
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
            value: config_id("gpt-5.5"),
        })
        .expect("set config option");
    assert_eq!(catalog_id(&catalog, "model"), Some("gpt-5.5"));

    runtime
        .cancel_session(&session.key())
        .expect("cancel session");
    prompt_handle
        .join()
        .expect("prompt thread")
        .expect("prompt cancelled cleanly");
    runtime
        .close_session(&session.key())
        .expect("close session");

    assert!(read_fixture_methods(&log_path)
        .iter()
        .any(|method| method == "session/set_config_option"));
}

#[test]
fn set_config_option_allows_an_agent_response_after_five_seconds() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    if !python3_available() {
        eprintln!("skipping ACP active-session runtime fixture: python3 not found");
        return;
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
                "slow-config-session".to_string(),
            ),
            (
                "OPENAIDE_ACP_FIXTURE_CONFIG_RESPONSE_DELAY".to_string(),
                "5.2".to_string(),
            ),
        ],
        secret_env: Vec::new(),
    });

    let session = runtime
        .start_session(start_request("task-slow-config", cwd_string()))
        .expect("start session");
    let catalog = runtime
        .set_session_config_option(AgentSessionSetConfigOptionRequest {
            agent_id: "codex".to_string(),
            session_id: session.session_id.clone(),
            config_id: "model".to_string(),
            value: config_id("gpt-5.5"),
        })
        .expect("slow config response should remain within the user-visible deadline");

    assert_eq!(catalog_id(&catalog, "model"), Some("gpt-5.5"));
    runtime
        .close_session(&session.key())
        .expect("close session");
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
                    agent_id: "codex".to_string(),
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
        .close_session(&session.key())
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
        .prompt(
            AgentPrompt {
                agent_id: "codex".to_string(),
                task_id: "task-duplicate-one".to_string(),
                session_id: session.session_id.clone(),
                text: "original remains active".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(CapturingEventSink::default()),
        )
        .expect("prompt original session after duplicate rejection");
    runtime
        .close_session(&session.key())
        .expect("close original session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/new",
            "session/prompt",
            "session/close"
        ]
    );
}

#[test]
fn auth_required_session_open_waits_for_explicit_user_authentication() {
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
        "agent authentication required: Authentication required. Open Settings and authenticate this Agent before starting a Task."
    );

    manager
        .close_session(&first.key())
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
        .close_session(&first.key())
        .expect("close first session");
    runtime
        .close_session(&second.key())
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
                    agent_id: "codex".to_string(),
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
        .cancel_session(&first.key())
        .expect("cancel first prompt");
    prompt_handle
        .join()
        .expect("prompt thread")
        .expect("prompt cancelled cleanly");
    runtime
        .close_session(&first.key())
        .expect("close first session");
    runtime
        .close_session(&second.key())
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
        .close_session(&started.key())
        .expect("close started session");
    runtime
        .close_session(&loaded.session.key())
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
fn load_session_reuses_the_native_session_worker() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((runtime, log_path)) =
        fixture_runtime_with_prompt_mode(&temp, "active-load-session", "load_replay")
    else {
        return;
    };
    let cwd = cwd_string();

    let started = runtime
        .start_session(start_request("task-active-load", cwd.clone()))
        .expect("start session");
    let loaded = runtime
        .load_session(AgentSessionLoad {
            agent_id: "codex".to_string(),
            task_id: "task-active-load".to_string(),
            session_id: started.session_id.clone(),
            cwd,
            model_id: None,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })
        .expect("reload history through active session worker");

    assert_eq!(loaded.session.session_id, started.session_id);
    assert!(matches!(
        &loaded.replayed_messages[..],
        [crate::protocol::model::NormalizedMessage::AgentMessage { parts, .. }]
            if matches!(
                &parts[..],
                [crate::protocol::model::AgentMessagePart::Text { text }]
                    if text == "replayed through active worker"
            )
    ));
    runtime
        .prompt(
            AgentPrompt {
                agent_id: "codex".to_string(),
                task_id: "task-active-load".to_string(),
                session_id: started.session_id.clone(),
                text: "continue after history reload".to_string(),
                attachments: Vec::new(),
                cancellation: TurnCancellation::new(),
            },
            Arc::new(CapturingEventSink::default()),
        )
        .expect("prompt after history reload");
    runtime
        .close_session(&started.key())
        .expect("close reloaded session");

    assert_eq!(
        read_fixture_methods(&log_path),
        [
            "initialize",
            "session/new",
            "session/load",
            "session/prompt",
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
        .close_session(&first.key())
        .expect("close first session");
    let second = runtime
        .start_session(start_request("task-reuse-after-close", cwd))
        .expect("second start");
    runtime
        .close_session(&second.key())
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
        .close_session(&loaded.session.key())
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
            agent_id: session.agent_id,
            session_id: session.session_id,
        })
        .expect("delete session");

    assert_eq!(
        read_fixture_methods(&log_path),
        ["initialize", "session/new", "session/delete"]
    );
}

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
