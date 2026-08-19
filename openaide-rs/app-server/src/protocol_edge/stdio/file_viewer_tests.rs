use super::ProtocolEdgeStdioDispatcher;
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::Store;
use crate::storage_runtime::StateRoot;
use openaide_app_server_protocol::methods::{CLIENT_INITIALIZE, FILE_VIEWER_OPEN};
use serde_json::{json, Value};

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

fn response(line: &str) -> Value {
    serde_json::from_str(line).expect("json response")
}

fn task_record(task_id: &str, workspace_root: String) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
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
        composer_history: Default::default(),
        message_queue: Default::default(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root,
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

#[test]
fn file_viewer_open_returns_a_snapshot_handle_through_protocol() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join("README.md"), "# Hello\n").unwrap();
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store
            .write_task(&task_record(
                "task-file-viewer",
                workspace.to_string_lossy().to_string(),
            ))
            .unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "open",
            "method": FILE_VIEWER_OPEN,
            "params": { "taskId": "task-file-viewer", "path": "README.md" }
        })
        .to_string(),
    );
    let snapshot = &response(&responses[0])["result"]["result"];

    assert_eq!(snapshot["kind"], "markdown");
    assert_eq!(snapshot["basename"], "README.md");
    assert_eq!(snapshot["text"], "# Hello\n");
    assert!(snapshot["handle"]
        .as_str()
        .unwrap()
        .starts_with("file-viewer-"));
}
