use std::time::{Duration, Instant};

use openaide_app_server_protocol::methods::{
    CLIENT_INITIALIZE, SETTINGS_GET_AGENT_DETAILS, TASK_ACQUIRE,
};
use serde_json::{json, Value};

use crate::projects::project_id_for_workspace;
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::Store;
use crate::storage_runtime::StateRoot;

use super::ProtocolEdgeStdioDispatcher;

#[test]
fn native_session_start_marks_the_agent_connected_in_settings() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let workspace_root = "/tmp/openaide-stdio-workspace/a";
    std::fs::create_dir_all(workspace_root).unwrap();
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&existing_task()).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    assert_eq!(
        codex_settings_status(&mut dispatcher, "before"),
        "disconnected"
    );

    let acquired = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "acquire",
            "method": TASK_ACQUIRE,
            "params": {
                "projectId": project_id_for_workspace(workspace_root),
                "agentId": "codex"
            }
        })
        .to_string(),
    );
    let acquired = response(&acquired[0]);
    assert!(
        acquired["result"]["result"]["task"]["task"]["taskId"].is_string(),
        "acquire should create a Prepared Task: {acquired}"
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut attempt = 0;
    loop {
        attempt += 1;
        if codex_settings_status(&mut dispatcher, &format!("poll-{attempt}")) == "connected" {
            return;
        }
        if Instant::now() >= deadline {
            panic!("Settings should show Codex as connected after Native Session start");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn init_request(id: &str, client_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": CLIENT_INITIALIZE,
        "params": {
            "clientInstanceId": client_id,
            "shell": { "kind": "web" },
            "requestedSurface": { "kind": "home" },
            "capabilities": {
                "protocol": ["permissionResponses", "questionResponses"]
            }
        },
        "meta": { "clientRequestId": "client-request-1" }
    })
    .to_string()
}

fn codex_settings_status(dispatcher: &mut ProtocolEdgeStdioDispatcher, id: &str) -> String {
    let lines = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": SETTINGS_GET_AGENT_DETAILS,
            "params": {}
        })
        .to_string(),
    );
    let payload = response(&lines[0]);
    payload["result"]["result"]["agents"]
        .as_array()
        .expect("agent settings details")
        .iter()
        .find(|agent| agent["agentId"] == "codex")
        .and_then(|agent| agent["status"].as_str())
        .expect("codex status")
        .to_string()
}

fn response(line: &str) -> Value {
    serde_json::from_str(line).expect("json")
}

fn existing_task() -> TaskRecord {
    let workspace_root = "/tmp/openaide-stdio-workspace/a";
    TaskRecord {
        task_id: "task-existing".to_string(),
        title: crate::storage::records::TaskTitleState::from_title(
            crate::storage::records::TaskTitle::new(
                "Task",
                crate::storage::records::TaskTitleSource::User,
            ),
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        pinned: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        permission_policy: Default::default(),
        composer_history: Default::default(),
        message_queue: Default::default(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace_root.to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Open,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        tombstoned: false,
        revision: 1,
        config_options_catalog: None,
        native_session_data_freshness: Default::default(),
        native_session_reload_requirement: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        context_usage: None,
        current_plan: None,
        completed_plan_message_id: None,
        last_turn_usage: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}
