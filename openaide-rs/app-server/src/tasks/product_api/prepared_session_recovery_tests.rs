use super::*;
use crate::agent::acp::{AcpAgentConfig, AcpAgentRuntime};

#[test]
fn acquiring_after_restart_recovers_an_unloaded_empty_codex_session() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().to_string_lossy().to_string();
    let state_root = temp.path().join("state");
    let store = Store::open(state_root.clone()).unwrap();
    let mut task = task_record("task-prepared", &workspace);
    task.lifecycle = test_new_task_lifecycle();
    task.agent_session_id = Some("empty-native-session".to_string());
    store.write_task(&task).unwrap();
    drop(store);

    // Replay the real adapter's response over ACP, not a preclassified mock
    // RuntimeError: the read-before-resume boundary changes the missing-session
    // error emitted after a native process restart.
    let script = temp.path().join("agent.mjs");
    std::fs::write(&script, EMPTY_SESSION_AGENT).unwrap();
    let traces = crate::agent::acp_trace::AcpTraceState::disabled(&state_root);
    traces.set_enabled(true).unwrap();
    let trace_directory = traces.status().directory;
    let agent = Arc::new(
        AcpAgentRuntime::new(AcpAgentConfig {
            agent_id: "codex".to_string(),
            command: "node".to_string(),
            args: vec![script.to_string_lossy().to_string()],
            env: Vec::new(),
            secret_env: Vec::new(),
        })
        .with_trace_state(traces),
    );
    let store = Store::open(state_root).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let acquired = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace),
            agent_id: "codex".into(),
            workspace_root: None,
        })
        .unwrap();
    assert_eq!(acquired.task.task_id.as_str(), "task-prepared");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = api
            .open_for_test(TaskOpenParams {
                task_id: acquired.task.task_id.clone(),
            })
            .unwrap();
        if !matches!(
            snapshot.preparation,
            TaskPreparationSnapshot::Preparing { .. }
        ) {
            assert!(
                matches!(
                    snapshot.send_capability.state,
                    TaskSendCapabilityState::Ready
                ),
                "the first acquire must recover without a user retry: {:?}",
                snapshot.preparation
            );
            assert!(!snapshot.task.has_messages);
            break;
        }
        assert!(Instant::now() < deadline, "preparation did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    let events: Vec<serde_json::Value> = std::fs::read_dir(trace_directory)
        .unwrap()
        .flat_map(|entry| {
            std::fs::read_to_string(entry.unwrap().path())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect::<Vec<serde_json::Value>>()
        })
        .collect();
    let failure = events
        .iter()
        .find(|event| event["event"] == "session/resume.error")
        .expect("failed resume must leave a terminal ACP trace before recovery");
    assert_eq!(failure["payload"]["code"], -32603);
    assert_eq!(
        failure["payload"]["data"]["details"],
        "thread not loaded: empty-native-session"
    );
}

const EMPTY_SESSION_AGENT: &str = r#"
import { createInterface } from 'node:readline';
for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    if (request.id === undefined) continue;
    const response = { jsonrpc: '2.0', id: request.id };
    switch (request.method) {
        case 'initialize':
            response.result = {
                protocolVersion: 1,
                agentCapabilities: { sessionCapabilities: { resume: {} } },
                authMethods: [],
            };
            break;
        case 'session/resume':
            response.error = {
                code: -32603, message: 'Internal error',
                data: { details: 'thread not loaded: empty-native-session' },
            };
            break;
        case 'session/new':
            response.result = { sessionId: 'replacement-native-session' };
            break;
        default:
            response.error = { code: -32601, message: 'Method not found' };
    }
    process.stdout.write(JSON.stringify(response) + '\n');
}
"#;

#[cfg(unix)]
#[test]
fn first_send_after_idle_process_expiry_recovers_empty_session_without_duplicate_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().to_string_lossy().into_owned();
    let store = Store::open(temp.path().join("state")).unwrap();
    let mut task = task_record("prepared-idle", &workspace);
    task.lifecycle = test_new_task_lifecycle();
    task.agent_session_id = Some("missing-empty-session".into());
    store.write_task(&task).unwrap();
    let script = temp.path().join("agent.mjs");
    let pid_file = temp.path().join("agent.pid");
    let prompts = temp.path().join("prompts.jsonl");
    std::fs::write(&script, IDLE_EMPTY_SESSION_AGENT).unwrap();
    let agent = Arc::new(
        AcpAgentRuntime::new(AcpAgentConfig {
            agent_id: "codex".into(),
            command: "node".into(),
            args: vec![
                script.to_string_lossy().into_owned(),
                pid_file.to_string_lossy().into_owned(),
                prompts.to_string_lossy().into_owned(),
            ],
            env: Vec::new(),
            secret_env: Vec::new(),
        })
        .with_process_idle_timeouts(Duration::from_millis(200), Duration::from_millis(800)),
    );
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store)),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let acquired = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace),
            agent_id: "codex".into(),
            workspace_root: None,
        })
        .unwrap();
    let snapshot = || {
        api.open_for_test(TaskOpenParams {
            task_id: acquired.task.task_id.clone(),
        })
        .unwrap()
    };
    wait_for_idle_recovery("preparation", || {
        matches!(
            snapshot().send_capability.state,
            TaskSendCapabilityState::Ready
        )
    });
    let original_pid = std::fs::read_to_string(&pid_file).unwrap();
    wait_for_idle_recovery("process expiration", || {
        !std::process::Command::new("kill")
            .args(["-0", &original_pid])
            .output()
            .is_ok_and(|result| result.status.success())
    });
    assert!(!snapshot().task.has_messages);

    api.send(TaskSendParams {
        task_id: acquired.task.task_id.clone(),
        queue_selection: None,
        message: ComposerMessage {
            text: Some("first message".into()),
            ..Default::default()
        },
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = snapshot();
        if current.task.has_messages && current.task.status == ProtocolTaskStatus::Idle {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "first Send did not complete: {:?}",
            current
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let delivered: Vec<serde_json::Value> = std::fs::read_to_string(&prompts)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        delivered.len(),
        1,
        "first Send must reach the replacement exactly once"
    );
    assert_eq!(delivered[0]["prompt"][0]["text"], "first message");
    assert_ne!(std::fs::read_to_string(&pid_file).unwrap(), original_pid);
    assert!(snapshot().chat.items.iter().any(|item| item
        .parts
        .iter()
        .any(|part| { matches!(part, MessagePart::Text { text } if text == "Recovered") })));
    api.shutdown().unwrap();
}

#[cfg(unix)]
fn wait_for_idle_recovery(stage: &str, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {stage}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
const IDLE_EMPTY_SESSION_AGENT: &str = r#"
import { createInterface } from 'node:readline';
import { writeFileSync, appendFileSync } from 'node:fs';
writeFileSync(process.argv[2], String(process.pid));
const sessions = new Set();
for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    if (request.id === undefined) continue;
    const response = { jsonrpc: '2.0', id: request.id };
    switch (request.method) {
        case 'initialize':
            response.result = { protocolVersion: 1, agentCapabilities: { sessionCapabilities: { resume: {} } }, authMethods: [] };
            break;
        case 'session/new': {
            const sessionId = `empty-${process.pid}`;
            sessions.add(sessionId);
            response.result = { sessionId };
            break;
        }
        case 'session/resume':
            if (sessions.has(request.params.sessionId)) response.result = {};
            else response.error = { code: -32603, message: 'Internal error', data: { details: `thread not loaded: ${request.params.sessionId}` } };
            break;
        case 'session/prompt':
            appendFileSync(process.argv[3], JSON.stringify(request.params) + '\n');
            process.stdout.write(JSON.stringify({ jsonrpc: '2.0', method: 'session/update', params: { sessionId: request.params.sessionId, update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'Recovered' } } } }) + '\n');
            response.result = { stopReason: 'end_turn' };
            break;
        default:
            response.error = { code: -32601, message: 'Method not found' };
    }
    process.stdout.write(JSON.stringify(response) + '\n');
}
"#;
