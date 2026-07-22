use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::*;
use crate::agent::mock::MockAgent;
use crate::protocol::jsonrpc::RpcId;

#[test]
fn invalid_json_returns_parse_error_response_with_null_id() {
    let storage = tempfile::tempdir().expect("temp storage");
    let runtime = Runtime::new_with_agent(storage.path().to_path_buf(), Arc::new(MockAgent))
        .expect("runtime");
    let mut dispatcher = ShellControlDispatcher::new(runtime);

    let responses = dispatcher.handle_line("{not-json");

    assert_eq!(responses.len(), 1);
    let response: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response["id"], Value::Null);
    assert_eq!(response["error"]["code"], -32700);
}

#[test]
fn invalid_jsonrpc_version_returns_invalid_request_with_original_id() {
    let storage = tempfile::tempdir().expect("temp storage");
    let runtime = Runtime::new_with_agent(storage.path().to_path_buf(), Arc::new(MockAgent))
        .expect("runtime");
    let mut dispatcher = ShellControlDispatcher::new(runtime);

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "1.0",
            "id": "wrong-version",
            "method": "runtime.health",
            "params": {}
        })
        .to_string(),
    );

    assert_eq!(responses.len(), 1);
    let response: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response["id"], "wrong-version");
    assert_eq!(response["error"]["code"], -32600);
}

#[test]
fn notifications_do_not_emit_responses_even_for_unknown_methods() {
    let storage = tempfile::tempdir().expect("temp storage");
    logging::init_file_logger(storage.path());
    let runtime = Runtime::new_with_agent(storage.path().to_path_buf(), Arc::new(MockAgent))
        .expect("runtime");
    let mut dispatcher = ShellControlDispatcher::new(runtime);

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "method": "unknown.notification",
            "params": {}
        })
        .to_string(),
    );

    assert!(responses.is_empty());
    let log_path = storage
        .path()
        .join("diagnostics")
        .join("logs")
        .join("openaide-app-server.jsonl");
    let logs = std::fs::read_to_string(log_path).expect("runtime log");
    assert!(logs.contains("\"event\":\"rpc_notification_failed\""));
    assert!(logs.contains("\"level\":\"warn\""));
}

#[test]
fn unknown_methods_return_method_not_found_response() {
    let storage = tempfile::tempdir().expect("temp storage");
    let runtime = Runtime::new_with_agent(storage.path().to_path_buf(), Arc::new(MockAgent))
        .expect("runtime");
    let mut dispatcher = ShellControlDispatcher::new(runtime);

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "unknown-method",
            "method": "does.not.exist",
            "params": {}
        })
        .to_string(),
    );

    assert_eq!(responses.len(), 1);
    let response: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response["id"], "unknown-method");
    assert_eq!(response["error"]["code"], -32601);
    assert_eq!(response["error"]["data"]["reason"], "method_not_found");
}

#[test]
fn host_responses_are_consumed_before_request_dispatch() {
    let storage = tempfile::tempdir().expect("temp storage");
    let runtime = Runtime::new_with_agent(storage.path().to_path_buf(), Arc::new(MockAgent))
        .expect("runtime");
    let (host_bridge, _requests) = HostBridge::channel();
    let mut dispatcher = ShellControlDispatcher::new_with_host(runtime, host_bridge);

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "unknown_host_request",
            "result": null
        })
        .to_string(),
    );

    assert!(responses.is_empty());
}

#[test]
fn malformed_host_response_shapes_still_return_invalid_request() {
    let storage = tempfile::tempdir().expect("temp storage");
    let runtime = Runtime::new_with_agent(storage.path().to_path_buf(), Arc::new(MockAgent))
        .expect("runtime");
    let (host_bridge, _requests) = HostBridge::channel();
    let mut dispatcher = ShellControlDispatcher::new_with_host(runtime, host_bridge);

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": { "not": "a valid id" },
            "result": null
        })
        .to_string(),
    );

    assert_eq!(responses.len(), 1);
    let response: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response["error"]["code"], -32600);
}

#[test]
fn dispatcher_unblocks_pending_host_bridge_request() {
    let storage = tempfile::tempdir().expect("temp storage");
    let runtime = Runtime::new_with_agent(storage.path().to_path_buf(), Arc::new(MockAgent))
        .expect("runtime");
    let (host_bridge, requests) = HostBridge::channel_with_timeout(Duration::from_secs(1));
    let mut dispatcher = ShellControlDispatcher::new_with_host(runtime, host_bridge.clone());

    let pending =
        std::thread::spawn(move || host_bridge.request("host/test", Some(json!({ "ok": true }))));
    let request = requests
        .recv_timeout(Duration::from_secs(1))
        .expect("host request should be emitted");
    assert_eq!(request.id, RpcId::String("host_1".to_string()));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": { "handled": true }
        })
        .to_string(),
    );

    assert!(responses.is_empty());
    assert_eq!(
        pending.join().expect("host request thread").unwrap(),
        json!({ "handled": true })
    );
}

#[test]
fn batch_requests_after_shutdown_are_rejected_without_mutating_storage() {
    let storage = tempfile::tempdir().expect("temp storage");
    let runtime = Runtime::new_with_agent(storage.path().to_path_buf(), Arc::new(MockAgent))
        .expect("runtime");
    let mut dispatcher = ShellControlDispatcher::new(runtime);

    let responses = dispatcher.handle_line(
        &json!([
            {"jsonrpc":"2.0","id":"shutdown","method":"runtime.shutdown","params":{}},
            {"jsonrpc":"2.0","id":"health-after-shutdown","method":"runtime.health","params":{}}
        ])
        .to_string(),
    );

    assert_eq!(responses.len(), 2);
    let shutdown_response: Value = serde_json::from_str(&responses[0]).unwrap();
    let health_response: Value = serde_json::from_str(&responses[1]).unwrap();
    assert_eq!(shutdown_response["result"], json!({}));
    assert_eq!(health_response["error"]["data"]["reason"], "not_ready");
    assert_eq!(
        std::fs::read_dir(storage.path().join("task-store-v1/tasks"))
            .unwrap()
            .count(),
        0
    );
}
