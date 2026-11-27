use std::cell::{Cell, RefCell};

use openaide_app_server_protocol::client::{RequestedSurface, ShellKind};
use openaide_app_server_protocol::envelopes::{
    ErrorEnvelope, RequestMeta, ResponseEnvelope, ResponseMeta,
};
use openaide_app_server_protocol::events::{AppServerEvent, AppServerEventPayload, EventScope};
use openaide_app_server_protocol::methods::{
    AGENT_AUTHENTICATE, AGENT_CANCEL_AUTHENTICATE, CLIENT_INITIALIZE,
};
use openaide_app_server_protocol::snapshot::{
    ClientSnapshot, ClientSnapshotScope, ProjectCollectionSnapshot, ProtocolVersion,
    ServerCapabilities, ServerSnapshot, StateRootSnapshot,
};
use serde_json::{json, Value};

use super::*;
use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::protocol_edge::outbox::{OutboundMessage, Outbox, ReplyRoute};
use crate::protocol_edge::{
    GatewayOutcome, GatewayResponse, InboundProtocolMessage, SharedRpcGateway,
};

/// A stand-in App Server: `respond` decides each message's outcome, which joins the fake's own
/// outbox through the same `push_outcome` the gateway uses.
struct FakeExchange<F> {
    outbox: Outbox,
    respond: RefCell<F>,
    live: bool,
    observed: Cell<bool>,
}

impl<F> ProtocolExchange for FakeExchange<F>
where
    F: FnMut(ConnectionId, InboundProtocolMessage) -> GatewayOutcome,
{
    fn dispatch(&self, connection_id: ConnectionId, message: InboundProtocolMessage) -> bool {
        let route = ReplyRoute::for_message(&message);
        let outcome = (self.respond.borrow_mut())(connection_id, message);
        self.outbox.push_outcome(outcome, route)
    }

    fn observe_link(&self, _connection_id: &ConnectionId) -> bool {
        self.observed.set(true);
        self.live
    }

    fn transfer<R>(
        &self,
        connection_id: &ConnectionId,
        write: impl FnOnce(Vec<OutboundMessage>) -> R,
    ) -> R {
        self.outbox.transfer(connection_id, write)
    }
}

fn exchange<F>(respond: F) -> FakeExchange<F>
where
    F: FnMut(ConnectionId, InboundProtocolMessage) -> GatewayOutcome,
{
    FakeExchange {
        outbox: Outbox::default(),
        respond: RefCell::new(respond),
        live: true,
        observed: Cell::new(false),
    }
}

fn noop() -> FakeExchange<impl FnMut(ConnectionId, InboundProtocolMessage) -> GatewayOutcome> {
    exchange(|_, _| GatewayOutcome::Noop)
}

fn http(body: &str, exchange: &impl ProtocolExchange) -> (u16, Value) {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        body,
        exchange,
    );
    let body = if response.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&response.body).unwrap()
    };
    (response.status, body)
}

fn client_response_body() -> String {
    json!({
        "jsonrpc": "2.0",
        "id": "server-request-1",
        "result": { "optionId": "allow-once" }
    })
    .to_string()
}

#[test]
fn product_request_requires_connection_id_header() {
    let response = handle_local_http_protocol(
        Some("Bearer token"),
        "token",
        None,
        &request("1", "client/initialize", json!({})),
        &noop(),
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
    let exchange = exchange(|connection_id, message| {
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
    });

    let (status, body) = http(
        &request(
            "1",
            "client/initialize",
            json!({"clientInstanceId": "client-1"}),
        ),
        &exchange,
    );

    assert_eq!(status, 200);
    assert!(body.as_array().is_some_and(|messages| messages.len() == 1));
    assert_eq!(body[0]["jsonrpc"], "2.0");
    assert_eq!(body[0]["id"], "1");
    assert_eq!(
        body[0]["result"]["result"]["server"]["serverId"],
        "server-1"
    );
}

#[test]
fn product_request_dispatches_a_notification_without_a_reply() {
    let exchange = exchange(|connection_id, message| {
        assert!(matches!(
            message,
            InboundProtocolMessage::ClientNotification { method, .. } if method == "client/example"
        ));
        GatewayOutcome::Respond {
            connection_id: connection_id.clone(),
            id: String::new(),
            response: GatewayResponse::Result(Value::Null),
            events: vec![event_delivery(
                &connection_id,
                app_event("cursor-1", "cursor-2"),
            )],
            server_requests: Vec::new(),
        }
    });

    let (status, body) = http(
        &json!({ "jsonrpc": "2.0", "method": "client/example", "params": {} }).to_string(),
        &exchange,
    );

    assert_eq!(status, 200);
    let messages = body.as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["method"], "app/event");
}

#[test]
fn product_transport_accepts_client_response_for_server_request() {
    let exchange = exchange(|connection_id, message| {
        assert_eq!(connection_id, ConnectionId::new("local-http:client-1"));
        assert!(matches!(
            message,
            InboundProtocolMessage::ClientResponse { request_id, .. }
                if request_id == "server-request-1"
        ));
        GatewayOutcome::Noop
    });

    let (status, body) = http(&client_response_body(), &exchange);

    assert_eq!(status, 200);
    assert!(body.as_array().is_some_and(Vec::is_empty));
}

#[test]
fn product_transport_returns_error_when_client_response_is_rejected() {
    let exchange = exchange(|connection_id, _message| GatewayOutcome::Respond {
        connection_id,
        id: "server-request-1".to_string(),
        response: GatewayResponse::Error(Box::new(ErrorEnvelope::new(
            openaide_app_server_protocol::errors::ProtocolError {
                code:
                    openaide_app_server_protocol::errors::ProtocolErrorCode::RequestAlreadyResolved,
                message: "Permission request is no longer answerable.".to_string(),
                recoverable: false,
                target: None,
            },
            ResponseMeta::default(),
        ))),
        events: Vec::new(),
        server_requests: Vec::new(),
    });

    let (status, body) = http(&client_response_body(), &exchange);

    assert_eq!(status, 200);
    assert_eq!(body[0]["id"], "server-request-1");
    assert_eq!(
        body[0]["error"]["error"]["message"],
        "Permission request is no longer answerable."
    );
}

#[test]
fn product_transport_answers_an_accepted_client_response_with_its_events_only() {
    let exchange = exchange(|connection_id, _message| GatewayOutcome::Respond {
        connection_id: connection_id.clone(),
        id: String::new(),
        response: GatewayResponse::Result(Value::Null),
        events: vec![event_delivery(
            &connection_id,
            app_event("cursor-2", "cursor-3"),
        )],
        server_requests: Vec::new(),
    });

    let (status, body) = http(&client_response_body(), &exchange);

    assert_eq!(status, 200);
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["params"]["cursor"], "cursor-3");
}

#[test]
fn product_transport_returns_everything_queued_for_the_connection_in_order() {
    let connection_id = ConnectionId::new("local-http:client-1");
    let exchange = exchange(|connection_id, _message| GatewayOutcome::Respond {
        connection_id,
        id: "1".to_string(),
        response: GatewayResponse::Result(json!({ "result": {} })),
        events: Vec::new(),
        server_requests: Vec::new(),
    });
    exchange.outbox.push(
        &connection_id,
        OutboundMessage::Event(Box::new(app_event("cursor-1", "cursor-2"))),
    );

    let (status, body) = http(&request("1", "task/list", json!({})), &exchange);

    assert_eq!(status, 200);
    assert_eq!(body[0]["params"]["cursor"], "cursor-2");
    assert_eq!(body[1]["id"], "1");
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[test]
fn product_request_without_a_reply_is_an_internal_error() {
    let (status, body) = http(&request("1", "task/list", json!({})), &noop());

    assert_eq!(status, 500);
    assert_eq!(body["code"], "request_produced_no_response");
    assert_eq!(body["id"], "1");
}

#[test]
fn product_transport_rejects_client_response_without_jsonrpc_version() {
    let (status, body) = http(
        &json!({
            "id": "server-request-1",
            "result": { "optionId": "allow-once" }
        })
        .to_string(),
        &noop(),
    );

    assert_eq!(status, 400);
    assert!(body["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("missing field `jsonrpc`"));
}

#[test]
fn product_transport_rejects_client_response_with_result_and_error() {
    let (status, body) = http(
        &json!({
            "jsonrpc": "2.0",
            "id": "server-request-1",
            "result": { "optionId": "allow-once" },
            "error": { "code": -32000, "message": "denied" }
        })
        .to_string(),
        &noop(),
    );

    assert_eq!(status, 400);
    assert!(body["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("method is required"));
}

#[test]
fn product_protocol_rejections_have_safe_reason_codes() {
    let cases = [
        ("{", "malformed_json"),
        (
            r#"{"jsonrpc":"2.0","id":true,"method":"task/list"}"#,
            "invalid_request_envelope",
        ),
        (
            r#"{"jsonrpc":"2.0","id":null,"method":"task/list"}"#,
            "invalid_request_id",
        ),
        (
            r#"{"jsonrpc":"1.0","id":"1","method":"task/list"}"#,
            "invalid_jsonrpc_version",
        ),
        (r#"{"jsonrpc":"2.0","id":"1"}"#, "missing_method"),
    ];

    for (body, expected_code) in cases {
        let (status, body) = http(body, &noop());

        assert_eq!(status, 400);
        assert_eq!(body["code"], expected_code);
    }
}

fn open_session(sessions: &ReliableSessionRegistry) -> String {
    let opened =
        handle_reliable_session_open(Some("Bearer token"), "token", Some("client-1"), sessions);
    let handshake: Value = serde_json::from_str(&opened.body).unwrap();
    handshake["sessionId"].as_str().unwrap().to_string()
}

fn upload(
    sessions: &ReliableSessionRegistry,
    session_id: &str,
    sequence: u64,
    message: Value,
    exchange: &impl ProtocolExchange,
) -> LocalHttpResponse {
    handle_reliable_session_upload(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        &json!({ "sessionId": session_id, "sequence": sequence, "message": message }).to_string(),
        sessions,
        exchange,
    )
}

fn poll(
    sessions: &ReliableSessionRegistry,
    session_id: &str,
    exchange: &impl ProtocolExchange,
) -> LocalHttpResponse {
    handle_reliable_session_poll(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        session_id,
        0,
        sessions,
        exchange,
    )
}

#[test]
fn reliable_upload_returns_no_rpc_messages_and_poll_delivers_the_response() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let session_id = open_session(&sessions);
    let exchange = exchange(|connection_id, message| {
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
    });

    let uploaded = upload(
        &sessions,
        &session_id,
        1,
        json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "task/list",
            "params": {}
        }),
        &exchange,
    );
    let polled = poll(&sessions, &session_id, &exchange);

    assert_eq!(uploaded.status, 204);
    assert!(uploaded.body.is_empty());
    assert!(exchange.observed.get());
    let batch: Value = serde_json::from_str(&polled.body).unwrap();
    assert_eq!(batch["frames"][0]["sequence"], 1);
    assert_eq!(batch["frames"][0]["message"]["id"], "request-1");
    assert_eq!(
        batch["frames"][0]["message"]["result"]["result"]["tasks"],
        json!([])
    );
}

#[test]
fn reliable_upload_accepts_a_notification_and_sends_no_reply() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let session_id = open_session(&sessions);
    let dispatched = Cell::new(false);
    let exchange = exchange(|_, message| {
        assert!(matches!(
            message,
            InboundProtocolMessage::ClientNotification { .. }
        ));
        dispatched.set(true);
        GatewayOutcome::Noop
    });

    let uploaded = upload(
        &sessions,
        &session_id,
        1,
        json!({ "jsonrpc": "2.0", "method": "client/example", "params": {} }),
        &exchange,
    );
    let polled = poll(&sessions, &session_id, &exchange);

    assert_eq!(uploaded.status, 204);
    assert!(dispatched.get());
    assert_eq!(polled.status, 204);
}

#[test]
fn reliable_session_carries_a_connections_messages_in_queue_order() {
    let connection_id = ConnectionId::new("local-http:client-1");
    let sessions = ReliableSessionRegistry::new("server-1");
    let session_id = open_session(&sessions);
    let exchange = exchange(|connection_id, _message| GatewayOutcome::Respond {
        connection_id,
        id: "request-1".to_string(),
        response: GatewayResponse::Result(json!({ "result": {} })),
        events: Vec::new(),
        server_requests: Vec::new(),
    });
    exchange.outbox.push(
        &connection_id,
        OutboundMessage::Event(Box::new(app_event("cursor-1", "cursor-2"))),
    );

    upload(
        &sessions,
        &session_id,
        1,
        json!({ "jsonrpc": "2.0", "id": "request-1", "method": "task/list", "params": {} }),
        &exchange,
    );
    exchange.outbox.push(
        &connection_id,
        OutboundMessage::Event(Box::new(app_event("cursor-2", "cursor-3"))),
    );
    let polled = poll(&sessions, &session_id, &exchange);

    let batch: Value = serde_json::from_str(&polled.body).unwrap();
    let frames = batch["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0]["message"]["params"]["cursor"], "cursor-2");
    assert_eq!(frames[1]["message"]["id"], "request-1");
    assert_eq!(frames[2]["message"]["params"]["cursor"], "cursor-3");
}

#[test]
fn reliable_poll_of_a_connection_without_a_client_ends_the_session() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let session_id = open_session(&sessions);
    let mut exchange = noop();
    exchange.live = false;

    assert_eq!(poll(&sessions, &session_id, &exchange).status, 410);
}

#[test]
fn reliable_upload_acknowledges_authenticate_before_cancel_completes_it() {
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let workflow = std::sync::Arc::new(CancelReleasesAuthentication::new(started_tx));
    let gateway = SharedRpcGateway::new(
        crate::protocol_edge::tests::gateway_with_agent_authenticate(workflow.clone()),
    );
    let handler = LocalHttpProtocolHandler::new(gateway, "token", "server-1");
    let opened = handler.handle(
        Some("Bearer token"),
        Some("client-1"),
        &json!({ "transport": "open" }).to_string(),
    );
    let handshake: Value = serde_json::from_str(&opened.body).unwrap();
    let session_id = handshake["sessionId"].as_str().unwrap().to_string();
    let initialized = handler.handle(
        Some("Bearer token"),
        Some("client-1"),
        &reliable_request(
            &session_id,
            1,
            "initialize",
            CLIENT_INITIALIZE,
            serde_json::to_value(crate::protocol_edge::tests::init_params("client-1")).unwrap(),
        ),
    );
    assert_eq!(initialized.status, 204);

    let authenticate_handler = handler.clone();
    let authenticate_session_id = session_id.clone();
    let (ack_tx, ack_rx) = std::sync::mpsc::channel();
    let authenticate_upload = std::thread::spawn(move || {
        let response = authenticate_handler.handle(
            Some("Bearer token"),
            Some("client-1"),
            &reliable_request(
                &authenticate_session_id,
                2,
                "authenticate",
                AGENT_AUTHENTICATE,
                json!({ "agentId": "codex", "methodId": "chat-gpt" }),
            ),
        );
        let _ = ack_tx.send(response);
    });
    started_rx
        .recv_timeout(crate::test_sync::WATCHDOG)
        .expect("authentication started");

    let acknowledgement = ack_rx.recv_timeout(crate::test_sync::WATCHDOG);
    if acknowledgement.is_err() {
        workflow.release();
    }
    authenticate_upload
        .join()
        .expect("authenticate upload thread");
    assert_eq!(
        acknowledgement
            .expect("authenticate upload was acknowledged while authentication was still running")
            .status,
        204,
    );

    let cancelled = handler.handle(
        Some("Bearer token"),
        Some("client-1"),
        &reliable_request(
            &session_id,
            3,
            "cancel-authenticate",
            AGENT_CANCEL_AUTHENTICATE,
            json!({ "agentId": "codex" }),
        ),
    );
    assert_eq!(cancelled.status, 204);
    workflow
        .cancelled()
        .recv_timeout(crate::test_sync::WATCHDOG)
        .expect("cancel reached the authentication workflow");
}

#[test]
fn reliable_upload_reports_an_invalid_connection_id() {
    let sessions = ReliableSessionRegistry::new("server-1");

    let response = handle_reliable_session_upload(
        Some("Bearer token"),
        "token",
        Some("invalid/connection"),
        "{}",
        &sessions,
        &noop(),
    );

    assert_eq!(response.status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&response.body).unwrap(),
        json!({"code": "invalid_connection_id"})
    );
}

#[test]
fn reliable_upload_reports_an_invalid_upload_envelope() {
    let sessions = ReliableSessionRegistry::new("server-1");

    let response = handle_reliable_session_upload(
        Some("Bearer token"),
        "token",
        Some("client-1"),
        "{}",
        &sessions,
        &noop(),
    );

    assert_eq!(response.status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&response.body).unwrap(),
        json!({"code": "invalid_upload_envelope"})
    );
}

#[test]
fn reliable_upload_preserves_the_nested_protocol_rejection_code() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let session_id = open_session(&sessions);

    let response = upload(
        &sessions,
        &session_id,
        1,
        json!({ "jsonrpc": "1.0", "id": "1", "method": "task/list" }),
        &noop(),
    );

    assert_eq!(response.status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&response.body).unwrap()["code"],
        "invalid_jsonrpc_version"
    );
}

#[test]
fn reliable_chunk_upload_reports_invalid_boundary_inputs() {
    let gateway = SharedRpcGateway::new(crate::protocol_edge::tests::initialized_gateway(
        "client-1",
        "local-http:client-1",
    ));
    let handler = LocalHttpProtocolHandler::new(gateway, "token", "server-1");
    let opened = handler.handle(
        Some("Bearer token"),
        Some("client-1"),
        &json!({ "transport": "open" }).to_string(),
    );
    let handshake: Value = serde_json::from_str(&opened.body).unwrap();
    let session_id = handshake["sessionId"].as_str().unwrap();
    let cases = [
        (
            Some("invalid/connection"),
            json!({"transport": "chunk"}).to_string(),
            "invalid_connection_id",
        ),
        (
            Some("client-1"),
            json!({"transport": "chunk"}).to_string(),
            "invalid_chunk_envelope",
        ),
        (
            Some("client-1"),
            json!({
                "transport": "chunk",
                "sessionId": session_id,
                "sequence": 1,
                "offset": 0,
                "totalSize": 1,
                "data": "not base64!"
            })
            .to_string(),
            "invalid_chunk_base64",
        ),
    ];

    for (connection_id, body, expected_code) in cases {
        let response = handler.handle(Some("Bearer token"), connection_id, &body);
        assert_eq!(response.status, 400);
        assert_eq!(
            serde_json::from_str::<Value>(&response.body).unwrap(),
            json!({"code": expected_code})
        );
    }
}

#[test]
fn reliable_poll_reports_an_invalid_connection_id() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let opened =
        handle_reliable_session_open(Some("Bearer token"), "token", Some("client-1"), &sessions);
    let handshake: Value = serde_json::from_str(&opened.body).unwrap();
    let session_id = handshake["sessionId"].as_str().unwrap();

    let response = handle_reliable_session_poll(
        Some("Bearer token"),
        "token",
        Some("invalid/connection"),
        session_id,
        0,
        &sessions,
        &noop(),
    );

    assert_eq!(response.status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&response.body).unwrap(),
        json!({"code": "invalid_connection_id"})
    );
}

#[test]
fn reliable_chunk_upload_reassembles_one_in_memory_client_frame() {
    use base64::Engine;

    let gateway = SharedRpcGateway::new(crate::protocol_edge::tests::initialized_gateway(
        "client-1",
        "local-http:client-1",
    ));
    let handler = LocalHttpProtocolHandler::new(gateway, "token", "server-1");
    let opened = handler.handle(
        Some("Bearer token"),
        Some("client-1"),
        &json!({ "transport": "open" }).to_string(),
    );
    let handshake: Value = serde_json::from_str(&opened.body).unwrap();
    let session_id = handshake["sessionId"].as_str().unwrap();
    let upload = json!({
        "transport": "send",
        "sessionId": session_id,
        "sequence": 1,
        "message": {
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "task/list",
            "params": {},
            "meta": RequestMeta::default(),
        }
    })
    .to_string();
    let split = upload.len() / 2;
    let chunks = [&upload.as_bytes()[..split], &upload.as_bytes()[split..]];

    for (index, chunk) in chunks.into_iter().enumerate() {
        let offset = if index == 0 { 0 } else { split };
        let response = handler.handle(
            Some("Bearer token"),
            Some("client-1"),
            &json!({
                "transport": "chunk",
                "sessionId": session_id,
                "sequence": 1,
                "offset": offset,
                "totalSize": upload.len(),
                "data": base64::engine::general_purpose::STANDARD.encode(chunk),
            })
            .to_string(),
        );
        assert_eq!(response.status, if index == 0 { 202 } else { 204 });
    }

    let poll = handler.poll_session(Some("Bearer token"), Some("client-1"), session_id, 0);
    let batch: Value = serde_json::from_str(&poll.body).unwrap();
    assert_eq!(batch["frames"][0]["message"]["id"], "request-1");
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

fn reliable_request(
    session_id: &str,
    sequence: u64,
    id: &str,
    method: &str,
    params: Value,
) -> String {
    json!({
        "transport": "send",
        "sessionId": session_id,
        "sequence": sequence,
        "message": {
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
            "meta": RequestMeta::default(),
        }
    })
    .to_string()
}

struct CancelReleasesAuthentication {
    started: std::sync::mpsc::Sender<()>,
    cancelled_rx: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    cancelled_tx: std::sync::mpsc::Sender<()>,
    state: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}

impl CancelReleasesAuthentication {
    fn new(started: std::sync::mpsc::Sender<()>) -> Self {
        let (cancelled_tx, cancelled_rx) = std::sync::mpsc::channel();
        Self {
            started,
            cancelled_rx: std::sync::Mutex::new(Some(cancelled_rx)),
            cancelled_tx,
            state: std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new())),
        }
    }

    fn release(&self) {
        let (cancelled, wake) = self.state.as_ref();
        *cancelled.lock().expect("cancel state") = true;
        wake.notify_all();
    }

    fn cancelled(&self) -> std::sync::mpsc::Receiver<()> {
        self.cancelled_rx
            .lock()
            .expect("cancel receiver")
            .take()
            .expect("cancel receiver is read once")
    }
}

impl crate::agent::product_api::AgentAuthenticateWorkflow for CancelReleasesAuthentication {
    fn authenticate(
        &self,
        params: openaide_app_server_protocol::agent::AgentAuthenticateParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentAuthenticateResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        let _ = self.started.send(());
        let (cancelled, wake) = self.state.as_ref();
        let mut cancelled = cancelled.lock().expect("cancel state");
        while !*cancelled {
            cancelled = wake.wait(cancelled).expect("cancel state");
        }
        Ok(
            openaide_app_server_protocol::agent::AgentAuthenticateResult {
                agent_id: params.agent_id,
                method_id: params.method_id,
                status: openaide_app_server_protocol::agent::AgentAuthenticateStatus::AwaitingUser,
                agents: openaide_app_server_protocol::snapshot::AgentCollectionSnapshot {
                    agents: Vec::new(),
                },
            },
        )
    }

    fn cancel_authenticate(
        &self,
        _params: openaide_app_server_protocol::agent::AgentCancelAuthenticateParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentCancelAuthenticateResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        self.release();
        let _ = self.cancelled_tx.send(());
        Ok(
            openaide_app_server_protocol::agent::AgentCancelAuthenticateResult {
                agents: openaide_app_server_protocol::snapshot::AgentCollectionSnapshot {
                    agents: Vec::new(),
                },
            },
        )
    }
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
