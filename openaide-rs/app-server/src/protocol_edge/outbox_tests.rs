use std::sync::mpsc;
use std::thread;

use openaide_app_server_protocol::envelopes::ServerRequestEnvelope;
use openaide_app_server_protocol::events::{AppServerEvent, AppServerEventPayload, EventScope};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use openaide_app_server_protocol::state::SubscriptionScope;
use serde_json::{json, Value};

use super::*;
use crate::client_lifecycle::Delivery;

fn connection(name: &str) -> ConnectionId {
    ConnectionId::new(name)
}

fn event(cursor: &str) -> AppServerEvent {
    AppServerEvent {
        subscription: SubscriptionScope::Task {
            task_id: "task-1".into(),
        },
        previous_cursor: "0".into(),
        cursor: cursor.into(),
        scope: EventScope::Task {
            state_root_id: "root-1".into(),
            task_id: "task-1".into(),
        },
        payload: AppServerEventPayload::TaskContentRemoved {
            task_id: "task-1".into(),
        },
    }
}

fn delivery(connection_id: &ConnectionId) -> Delivery {
    Delivery {
        client_instance_id: "client-1".into(),
        connection_id: connection_id.clone(),
    }
}

fn event_for(connection_id: &ConnectionId, cursor: &str) -> GatewayEventDelivery {
    GatewayEventDelivery {
        delivery: delivery(connection_id),
        event: event(cursor),
    }
}

fn server_request_for(connection_id: &ConnectionId, id: &str) -> ServerRequestDelivery {
    ServerRequestDelivery {
        delivery: delivery(connection_id),
        envelope: ServerRequestEnvelope::new(
            id.into(),
            PendingRequestScope::Client {
                client_instance_id: "client-1".into(),
            },
            "client/example",
            json!({}),
        ),
    }
}

fn reply(id: &str) -> OutboundMessage {
    OutboundMessage::Reply {
        id: Value::String(id.to_string()),
        response: GatewayResponse::Result(json!({ "id": id })),
    }
}

fn outcome(
    connection_id: &ConnectionId,
    response: GatewayResponse,
    events: Vec<GatewayEventDelivery>,
    server_requests: Vec<ServerRequestDelivery>,
) -> GatewayOutcome {
    GatewayOutcome::Respond {
        connection_id: connection_id.clone(),
        id: "1".to_string(),
        response,
        events,
        server_requests,
    }
}

fn error_response() -> GatewayResponse {
    GatewayResponse::Error(Box::new(
        openaide_app_server_protocol::envelopes::ErrorEnvelope::new(
            openaide_app_server_protocol::errors::ProtocolError {
                code: openaide_app_server_protocol::errors::ProtocolErrorCode::InvalidRequest,
                message: "rejected".to_string(),
                recoverable: false,
                target: None,
            },
            Default::default(),
        ),
    ))
}

fn drain(outbox: &Outbox, connection_id: &ConnectionId) -> Vec<OutboundMessage> {
    outbox.transfer(connection_id, |messages| messages)
}

#[test]
fn a_connection_receives_its_messages_in_push_order() {
    let outbox = Outbox::default();
    let first = connection("conn-1");
    let second = connection("conn-2");

    outbox.push(&first, reply("a"));
    outbox.push(&second, reply("b"));
    outbox.push(&first, OutboundMessage::Event(Box::new(event("1"))));

    assert_eq!(
        drain(&outbox, &first),
        vec![reply("a"), OutboundMessage::Event(Box::new(event("1")))]
    );
    assert_eq!(drain(&outbox, &second), vec![reply("b")]);
    assert!(drain(&outbox, &first).is_empty());
}

#[test]
fn a_reply_follows_everything_its_request_published() {
    let outbox = Outbox::default();
    let requester = connection("conn-1");
    let other = connection("conn-2");

    let replied = outbox.push_outcome(
        outcome(
            &requester,
            GatewayResponse::Result(json!({ "id": "1" })),
            vec![event_for(&requester, "1"), event_for(&other, "1")],
            vec![server_request_for(&requester, "server-request-1")],
        ),
        ReplyRoute::Request(Value::String("1".to_string())),
    );

    assert!(replied);
    let messages = drain(&outbox, &requester);
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0], OutboundMessage::Event(Box::new(event("1"))));
    assert_eq!(messages[1].server_request_method(), Some("client/example"));
    assert_eq!(messages[2], reply("1"));
    // A request's publication reaches every subscriber, not only the requester.
    assert_eq!(
        drain(&outbox, &other),
        vec![OutboundMessage::Event(Box::new(event("1")))]
    );
}

#[test]
fn an_answer_to_a_server_request_is_replied_to_only_when_rejected() {
    let outbox = Outbox::default();
    let client = connection("conn-1");
    let route = || ReplyRoute::Rejection(Value::String("server-request-1".to_string()));

    let accepted = outbox.push_outcome(
        outcome(
            &client,
            GatewayResponse::Result(json!({})),
            vec![event_for(&client, "1")],
            Vec::new(),
        ),
        route(),
    );
    let rejected = outbox.push_outcome(
        outcome(&client, error_response(), Vec::new(), Vec::new()),
        route(),
    );

    assert!(!accepted);
    assert!(rejected);
    assert_eq!(
        drain(&outbox, &client),
        vec![
            OutboundMessage::Event(Box::new(event("1"))),
            OutboundMessage::Reply {
                id: Value::String("server-request-1".to_string()),
                response: error_response(),
            },
        ]
    );
}

#[test]
fn a_notification_is_never_replied_to() {
    let outbox = Outbox::default();
    let client = connection("conn-1");

    let replied = outbox.push_outcome(
        outcome(
            &client,
            error_response(),
            vec![event_for(&client, "1")],
            Vec::new(),
        ),
        ReplyRoute::None,
    );

    assert!(!replied);
    assert_eq!(
        drain(&outbox, &client),
        vec![OutboundMessage::Event(Box::new(event("1")))]
    );
    assert!(!outbox.push_outcome(GatewayOutcome::Noop, ReplyRoute::None));
}

#[test]
fn a_push_during_a_write_waits_for_the_next_transfer() {
    let outbox = Outbox::default();
    let client = connection("conn-1");
    outbox.push(&client, reply("a"));

    let written = outbox.transfer(&client, |messages| {
        outbox.push(&client, reply("b"));
        messages
    });

    assert_eq!(written, vec![reply("a")]);
    assert_eq!(drain(&outbox, &client), vec![reply("b")]);
}

#[test]
fn two_transfers_for_one_connection_never_interleave() {
    let outbox = Outbox::default();
    let client = connection("conn-1");
    outbox.push(&client, reply("a"));
    let (writing_tx, writing_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (wire_tx, wire_rx) = mpsc::channel();

    let first = {
        let outbox = outbox.clone();
        let client = client.clone();
        let wire_tx = wire_tx.clone();
        thread::spawn(move || {
            outbox.transfer(&client, |messages| {
                writing_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                for message in messages {
                    wire_tx.send(message).unwrap();
                }
            });
        })
    };
    writing_rx.recv().unwrap();
    outbox.push(&client, reply("b"));
    let second = {
        let outbox = outbox.clone();
        let client = client.clone();
        thread::spawn(move || {
            outbox.transfer(&client, |messages| {
                for message in messages {
                    wire_tx.send(message).unwrap();
                }
            });
        })
    };
    // The second writer cannot drain "b" ahead of "a" while the first holds the transfer.
    assert!(wire_rx
        .recv_timeout(crate::test_sync::ABSENCE_WINDOW)
        .is_err());
    release_tx.send(()).unwrap();
    first.join().unwrap();
    second.join().unwrap();

    assert_eq!(
        wire_rx.iter().collect::<Vec<_>>(),
        vec![reply("a"), reply("b")]
    );
}

#[test]
fn discarding_a_connection_drops_its_undelivered_messages() {
    let outbox = Outbox::default();
    let client = connection("conn-1");
    outbox.push(&client, reply("a"));

    outbox.discard(&client, "client_detached");

    assert!(outbox.queued(&client).is_empty());
    assert!(drain(&outbox, &client).is_empty());
}

#[test]
fn an_idle_connection_leaves_no_queue_behind() {
    let outbox = Outbox::default();
    let client = connection("conn-1");
    outbox.push(&client, reply("a"));

    drain(&outbox, &client);
    drain(&outbox, &connection("never-pushed"));

    assert!(outbox.connections.lock().unwrap().is_empty());
}
