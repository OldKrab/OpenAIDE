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
    let agent = Arc::new(AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "node".to_string(),
        args: vec![script.to_string_lossy().to_string()],
        env: Vec::new(),
        secret_env: Vec::new(),
    }));
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
