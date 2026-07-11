use openaide_app_server_protocol::client::{
    ClientProbeLifecycle, ClientProbeResult, APP_SERVER_PROTOCOL_VERSION,
};
use openaide_app_server_protocol::envelopes::{ResponseEnvelope, ResponseMeta};
use serde_json::{json, Value};

use super::*;

#[test]
fn authorized_client_probe_routes_to_gateway_response() {
    let response = handle_local_http_probe(
        Some("Bearer token"),
        "token",
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
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], "client_probe");
    assert_eq!(body["result"]["result"]["stateRootFingerprint"], "root-a");
}

#[test]
fn auth_is_required_before_protocol_parsing() {
    let missing = handle_local_http_probe(None, "token", "{not-json", |_| GatewayOutcome::Noop);
    let invalid = handle_local_http_probe(Some("Bearer wrong"), "token", "{not-json", |_| {
        GatewayOutcome::Noop
    });

    assert_eq!(missing.status, 401);
    assert!(missing.body.is_empty());
    assert_eq!(invalid.status, 403);
    assert!(invalid.body.is_empty());
}

#[test]
fn malformed_jsonrpc_returns_protocol_error_body() {
    let response = handle_local_http_probe(Some("Bearer token"), "token", "{not-json", |_| {
        GatewayOutcome::Noop
    });

    assert_eq!(response.status, 400);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body["error"]["error"]["code"], "invalidRequest");
}

#[test]
fn rejects_non_probe_methods() {
    let response = handle_local_http_probe(
        Some("Bearer token"),
        "token",
        &json!({
            "jsonrpc": "2.0",
            "id": "not_probe",
            "method": "client/initialize",
            "params": {}
        })
        .to_string(),
        |_| GatewayOutcome::Noop,
    );

    assert_eq!(response.status, 400);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert!(body["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("client/probe"));
}
