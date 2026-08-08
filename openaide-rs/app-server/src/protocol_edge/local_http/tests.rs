use openaide_app_server_protocol::client::{
    ClientProbeLifecycle, ClientProbeResult, APP_SERVER_PROTOCOL_VERSION,
};
use openaide_app_server_protocol::envelopes::{ResponseEnvelope, ResponseMeta};
use serde_json::{json, Value};
use std::sync::mpsc;

use super::*;
use crate::protocol_edge::stdio::ProtocolEdgeStdioDispatcher;
use crate::storage_runtime::StateRoot;

#[test]
fn authorized_client_probe_routes_to_gateway_response() {
    let response = handle_local_http_probe(
        Some("Bearer token"),
        "token",
        "replacement-token",
        &json!({
            "jsonrpc": "2.0",
            "id": "client_probe",
            "method": CLIENT_PROBE,
            "params": {}
        })
        .to_string(),
        |_message| GatewayOutcome::Respond {
            connection_id: ConnectionId::new("local-http-probe"),
            id: "client_probe".to_string(),
            response: GatewayResponse::Result(
                serde_json::to_value(ResponseEnvelope::new(
                    ClientProbeResult {
                        state_root_fingerprint: "root-a".to_string(),
                        protocol_version: APP_SERVER_PROTOCOL_VERSION.to_string(),
                        app_version: "0.1.0".to_string(),
                        lifecycle: ClientProbeLifecycle::Running,
                    },
                    ResponseMeta::default(),
                ))
                .unwrap(),
            ),
            events: Vec::new(),
            server_requests: Vec::new(),
        },
        || Ok(()),
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], "client_probe");
    assert_eq!(body["result"]["result"]["stateRootFingerprint"], "root-a");
}

#[test]
fn auth_is_required_before_protocol_parsing() {
    let missing = handle_local_http_probe(
        None,
        "token",
        "replacement-token",
        "{not-json",
        |_| GatewayOutcome::Noop,
        || Ok(()),
    );
    let invalid = handle_local_http_probe(
        Some("Bearer wrong"),
        "token",
        "replacement-token",
        "{not-json",
        |_| GatewayOutcome::Noop,
        || Ok(()),
    );

    assert_eq!(missing.status, 401);
    assert!(missing.body.is_empty());
    assert_eq!(invalid.status, 403);
    assert!(invalid.body.is_empty());
}

#[test]
fn malformed_jsonrpc_returns_protocol_error_body() {
    let response = handle_local_http_probe(
        Some("Bearer token"),
        "token",
        "replacement-token",
        "{not-json",
        |_| GatewayOutcome::Noop,
        || Ok(()),
    );

    assert_eq!(response.status, 400);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body["error"]["error"]["code"], "invalidRequest");
}

#[test]
fn rejects_non_probe_methods() {
    let response = handle_local_http_probe(
        Some("Bearer token"),
        "token",
        "replacement-token",
        &json!({
            "jsonrpc": "2.0",
            "id": "not_probe",
            "method": "client/initialize",
            "params": {}
        })
        .to_string(),
        |_| GatewayOutcome::Noop,
        || Ok(()),
    );

    assert_eq!(response.status, 400);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert!(body["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("client/probe"));
}

#[test]
fn expired_last_client_requests_app_server_shutdown() {
    let state_dir = tempfile::TempDir::new().expect("state dir");
    let state_root = StateRoot::resolve(state_dir.path()).expect("state root");
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let gateway = dispatcher.shared_gateway();
    let (shutdown_sender, shutdown_receiver) = mpsc::channel();
    let handler = LocalHttpAppHandler::new(
        gateway.clone(),
        "token",
        "server-1",
        "replacement-token",
        shutdown_sender,
    );
    let connection_id = "vscode-connection-1";

    let initialized = handler.handle(
        Some("Bearer token"),
        Some(connection_id),
        &json!({
            "jsonrpc": "2.0",
            "id": "initialize",
            "method": "client/initialize",
            "params": {
                "clientInstanceId": "vscode-host-1",
                "shell": { "kind": "vscodeExtension", "name": "OpenAIDE" },
                "requestedSurface": { "kind": "home" },
                "workspaceRoots": []
            }
        })
        .to_string(),
    );
    assert_eq!(initialized.status, 200);

    let now = AppServerTime::now();
    assert!(gateway
        .expire_inactive_clients(AppServerTime(now.0 + 30_001))
        .is_empty());
    assert_eq!(
        gateway
            .expire_inactive_clients(AppServerTime(now.0 + 40_002))
            .len(),
        1
    );

    let heartbeat = handler.handle(
        Some("Bearer token"),
        Some(connection_id),
        &json!({
            "jsonrpc": "2.0",
            "id": "wake-heartbeat",
            "method": "client/heartbeat",
            "params": {}
        })
        .to_string(),
    );

    assert_eq!(heartbeat.status, 200);
    assert!(shutdown_receiver.try_recv().is_ok());
}
