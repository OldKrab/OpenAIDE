use openaide_app_server_protocol::client::{RequestedSurface, ShellKind};
use openaide_app_server_protocol::envelopes::{
    ErrorEnvelope, RequestMeta, ResponseEnvelope, ResponseMeta,
};
use openaide_app_server_protocol::events::{AppServerEvent, AppServerEventPayload, EventScope};
use openaide_app_server_protocol::snapshot::{
    ClientSnapshot, ClientSnapshotScope, ProjectCollectionSnapshot, ProtocolVersion,
    ServerCapabilities, ServerSnapshot, StateRootSnapshot,
};
use serde_json::{json, Value};

use super::*;
use crate::client_lifecycle::ConnectionId;
use crate::protocol_edge::{GatewayResponse, InboundProtocolMessage};

#[test]
fn product_request_requires_connection_id_header() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        None,
        &request("1", "client/initialize", json!({})),
        |_, _| GatewayOutcome::Noop,
        |_| Vec::new(),
    );

    assert_eq!(response.status, 400);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body["error"]["error"]["code"], "invalidRequest");
    assert!(body["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("X-OpenAIDE-Connection-Id"));
}

#[test]
fn product_request_dispatches_with_stable_local_http_connection_id() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &request(
            "1",
            "client/initialize",
            json!({"clientInstanceId": "client-1"}),
        ),
        |connection_id, message| {
            assert_eq!(connection_id, ConnectionId::new("local-http:client-1"));
            assert!(matches!(
                message,
                InboundProtocolMessage::ClientRequest { method, .. } if method == "client/initialize"
            ));
            GatewayOutcome::Respond {
                connection_id,
                id: "1".to_string(),
                response: GatewayResponse::Result(
                    serde_json::to_value(ResponseEnvelope::new(
                        client_snapshot(),
                        ResponseMeta::default(),
                    ))
                    .unwrap(),
                ),
                events: Vec::new(),
                server_requests: Vec::new(),
            }
        },
        |_| Vec::new(),
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert!(body.as_array().is_some_and(|messages| messages.len() == 1));
    assert_eq!(body[0]["jsonrpc"], "2.0");
    assert_eq!(body[0]["id"], "1");
    assert_eq!(
        body[0]["result"]["result"]["server"]["serverId"],
        "server-1"
    );
}

#[test]
fn product_request_rejects_notifications() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({
            "jsonrpc": "2.0",
            "method": "state/subscribe",
            "params": {}
        })
        .to_string(),
        |_, _| GatewayOutcome::Noop,
        |_| Vec::new(),
    );

    assert_eq!(response.status, 400);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert!(body["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("notifications"));
}

#[test]
fn product_transport_accepts_client_response_for_server_request() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({
            "jsonrpc": "2.0",
            "id": "server-request-1",
            "result": { "optionId": "allow-once" }
        })
        .to_string(),
        |connection_id, message| {
            assert_eq!(connection_id, ConnectionId::new("local-http:client-1"));
            assert!(matches!(
                message,
                InboundProtocolMessage::ClientResponse { request_id, .. }
                    if request_id == "server-request-1"
            ));
            GatewayOutcome::Noop
        },
        |_| Vec::new(),
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert!(body.as_array().is_some_and(Vec::is_empty));
}

#[test]
fn product_transport_returns_error_when_client_response_is_rejected() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({
            "jsonrpc": "2.0",
            "id": "server-request-1",
            "result": { "optionId": "allow-once" }
        })
        .to_string(),
        |connection_id, message| {
            assert_eq!(connection_id, ConnectionId::new("local-http:client-1"));
            assert!(matches!(
                message,
                InboundProtocolMessage::ClientResponse { request_id, .. }
                    if request_id == "server-request-1"
            ));
            GatewayOutcome::Respond {
                connection_id,
                id: "server-request-1".to_string(),
                response: GatewayResponse::Error(ErrorEnvelope::new(
                    openaide_app_server_protocol::errors::ProtocolError {
                        code: openaide_app_server_protocol::errors::ProtocolErrorCode::RequestAlreadyResolved,
                        message: "Permission request is no longer answerable.".to_string(),
                        recoverable: false,
                        target: None,
                    },
                    ResponseMeta::default(),
                )),
                events: Vec::new(),
                server_requests: Vec::new(),
            }
        },
        |_| Vec::new(),
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body[0]["id"], "server-request-1");
    assert_eq!(
        body[0]["error"]["error"]["message"],
        "Permission request is no longer answerable."
    );
}

#[test]
fn product_transport_returns_queued_events_with_client_response() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({
            "jsonrpc": "2.0",
            "id": "server-request-1",
            "result": { "optionId": "allow-once" }
        })
        .to_string(),
        |connection_id, _message| {
            assert_eq!(connection_id, ConnectionId::new("local-http:client-1"));
            GatewayOutcome::Noop
        },
        |connection_id| {
            vec![crate::protocol_edge::GatewayEventDelivery {
                delivery: crate::client_lifecycle::Delivery {
                    client_instance_id: "client-1".into(),
                    connection_id: connection_id.clone(),
                },
                event: app_event("cursor-1", "cursor-2"),
            }]
        },
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body[0]["method"], "app/event");
    assert_eq!(
        body[0]["params"]["payload"]["kind"],
        "projectCollectionUpdated"
    );
}

#[test]
fn product_transport_returns_queued_events_before_client_response_events() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({
            "jsonrpc": "2.0",
            "id": "server-request-1",
            "result": { "optionId": "allow-once" }
        })
        .to_string(),
        |connection_id, _message| {
            assert_eq!(connection_id, ConnectionId::new("local-http:client-1"));
            GatewayOutcome::Respond {
                connection_id: connection_id.clone(),
                id: String::new(),
                response: crate::protocol_edge::GatewayResponse::Result(serde_json::Value::Null),
                events: vec![event_delivery(
                    &connection_id,
                    app_event("cursor-2", "cursor-3"),
                )],
                server_requests: Vec::new(),
            }
        },
        |connection_id| {
            vec![event_delivery(
                connection_id,
                app_event("cursor-1", "cursor-2"),
            )]
        },
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body[0]["params"]["cursor"], "cursor-2");
    assert_eq!(body[1]["params"]["cursor"], "cursor-3");
}

#[test]
fn product_transport_rejects_client_response_without_jsonrpc_version() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({
            "id": "server-request-1",
            "result": { "optionId": "allow-once" }
        })
        .to_string(),
        |_, _| GatewayOutcome::Noop,
        |_| Vec::new(),
    );

    assert_eq!(response.status, 400);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert!(body["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("missing field `jsonrpc`"));
}

#[test]
fn product_transport_rejects_client_response_with_result_and_error() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({
            "jsonrpc": "2.0",
            "id": "server-request-1",
            "result": { "optionId": "allow-once" },
            "error": { "code": -32000, "message": "denied" }
        })
        .to_string(),
        |_, _| GatewayOutcome::Noop,
        |_| Vec::new(),
    );

    assert_eq!(response.status, 400);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert!(body["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("method is required"));
}

fn request(id: &str, method: &str, params: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
        "meta": RequestMeta::default(),
    })
    .to_string()
}

fn client_snapshot() -> ClientSnapshot {
    ClientSnapshot {
        cursor: "cursor-1".into(),
        server: ServerSnapshot {
            server_id: "server-1".into(),
            protocol_version: ProtocolVersion::V1,
            capabilities: ServerCapabilities::default(),
        },
        state_root: StateRootSnapshot {
            state_root_id: "root-1".into(),
        },
        client: ClientSnapshotScope {
            client_instance_id: "client-1".into(),
            shell_kind: ShellKind::Web,
            surface: RequestedSurface::Home,
        },
        projects: None,
        agents: None,
        tasks: None,
        active_task: None,
        settings: None,
        pending_requests: Vec::new(),
    }
}

fn app_event(previous_cursor: &str, cursor: &str) -> AppServerEvent {
    AppServerEvent {
        previous_cursor: previous_cursor.into(),
        cursor: cursor.into(),
        scope: EventScope::StateRoot {
            state_root_id: "root-1".into(),
        },
        payload: AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
                active_project_id: None,
            },
        },
    }
}

fn event_delivery(
    connection_id: &ConnectionId,
    event: AppServerEvent,
) -> crate::protocol_edge::GatewayEventDelivery {
    crate::protocol_edge::GatewayEventDelivery {
        delivery: crate::client_lifecycle::Delivery {
            client_instance_id: "client-1".into(),
            connection_id: connection_id.clone(),
        },
        event,
    }
}
