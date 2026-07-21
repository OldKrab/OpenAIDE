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
use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::protocol_edge::{GatewayResponse, InboundProtocolMessage, SharedRpcGateway};

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
                response: GatewayResponse::Error(Box::new(ErrorEnvelope::new(
                    openaide_app_server_protocol::errors::ProtocolError {
                        code: openaide_app_server_protocol::errors::ProtocolErrorCode::RequestAlreadyResolved,
                        message: "Permission request is no longer answerable.".to_string(),
                        recoverable: false,
                        target: None,
                    },
                    ResponseMeta::default(),
                ))),
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
fn product_transport_leaves_queued_events_for_the_push_channel_after_client_response() {
    let mut drained_events = false;
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
            drained_events = true;
            vec![crate::protocol_edge::GatewayEventDelivery {
                delivery: crate::client_lifecycle::Delivery {
                    client_instance_id: "client-1".into(),
                    connection_id: connection_id.clone(),
                    request_capabilities: Vec::new(),
                },
                event: app_event("cursor-1", "cursor-2"),
            }]
        },
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(body, json!([]));
    assert!(!drained_events);
}

#[test]
fn product_transport_returns_only_events_caused_by_a_client_response() {
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
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["params"]["cursor"], "cursor-3");
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

#[test]
fn reliable_upload_returns_no_rpc_messages_and_poll_delivers_the_response() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let opened =
        handle_reliable_session_open(Some("Bearer token"), "token", Some("client-1"), &sessions);
    let handshake: Value = serde_json::from_str(&opened.body).unwrap();
    let session_id = handshake["sessionId"].as_str().unwrap();

    let upload = handle_reliable_session_upload(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({
            "sessionId": session_id,
            "sequence": 1,
            "message": {
                "jsonrpc": "2.0",
                "id": "request-1",
                "method": "task/list",
                "params": {}
            }
        })
        .to_string(),
        &sessions,
        |connection_id, message| {
            assert!(matches!(
                message,
                InboundProtocolMessage::ClientRequest { .. }
            ));
            GatewayOutcome::Respond {
                connection_id,
                id: "request-1".to_string(),
                response: GatewayResponse::Result(json!({"result": {"tasks": []}})),
                events: Vec::new(),
                server_requests: Vec::new(),
            }
        },
    );
    let mut observed_liveness = false;
    let poll = handle_reliable_session_poll(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        session_id,
        0,
        &sessions,
        |connection_id| {
            assert_eq!(connection_id, &ConnectionId::new("local-http:client-1"));
            observed_liveness = true;
            Some((Vec::new(), Vec::new()))
        },
    );

    assert_eq!(upload.status, 204);
    assert!(upload.body.is_empty());
    assert!(observed_liveness);
    let batch: Value = serde_json::from_str(&poll.body).unwrap();
    assert_eq!(batch["frames"][0]["sequence"], 1);
    assert_eq!(batch["frames"][0]["message"]["id"], "request-1");
    assert_eq!(
        batch["frames"][0]["message"]["result"]["result"]["tasks"],
        json!([])
    );
}

#[test]
fn reliable_poll_keeps_the_initialized_product_client_live() {
    let connection_id = ConnectionId::new("local-http:client-1");
    let gateway = SharedRpcGateway::new(crate::protocol_edge::tests::initialized_gateway(
        "client-1",
        "local-http:client-1",
    ));
    let handler = LocalHttpProtocolHandler::new(gateway.clone(), "token", "server-1");
    let opened = handler.handle(
        Some("Bearer token"),
        Some("client-1"),
        &json!({ "transport": "open" }).to_string(),
    );
    let handshake: Value = serde_json::from_str(&opened.body).unwrap();
    let session_id = handshake["sessionId"].as_str().unwrap();
    let before_poll = AppServerTime::now();

    let poll = handler.poll_session(Some("Bearer token"), Some("client-1"), session_id, 0);
    gateway.expire_inactive_clients(AppServerTime(before_poll.0 + 1));

    assert_eq!(poll.status, 204);
    assert!(gateway.connection_is_initialized(&connection_id));
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
        new_task_defaults: Default::default(),
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
        subscription: openaide_app_server_protocol::state::SubscriptionScope::Projects,
        previous_cursor: previous_cursor.into(),
        cursor: cursor.into(),
        scope: EventScope::StateRoot {
            state_root_id: "root-1".into(),
        },
        payload: AppServerEventPayload::ProjectCollectionUpdated {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
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
            request_capabilities: Vec::new(),
        },
        event,
    }
}
