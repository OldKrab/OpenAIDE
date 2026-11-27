//! Task entity subscriptions at the gateway (ADR-0086): the reply precedes the subscription's
//! notifications, and a deleted Task ends its subscriptions with a final `removed` notification.

use openaide_app_server_protocol::events::TaskChanges;

use super::*;
use crate::protocol_edge::outbox::OutboundMessage;

fn open_task(task_id: &str) -> crate::storage::records::TaskRecord {
    let mut task = client_new_task_record(task_id, "client-1");
    task.lifecycle = crate::storage::records::TaskLifecycle::Open;
    task
}

fn subscribe(
    gateway: &mut RpcGateway,
    task_id: &str,
    known_version: Option<u64>,
) -> serde_json::Value {
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "subscribe",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from(task_id),
                },
                known_version,
            },
        ),
        AppServerTime(10),
    ))["result"]
        .clone()
}

fn removed_update(task_id: &str, revision: u64) -> TaskUpdate {
    TaskUpdate {
        task_id: task_id.to_string(),
        revision,
        kind: TaskUpdateKind::Changed(Box::new(CommittedTaskChange {
            changes: TaskChanges {
                removed: true,
                ..TaskChanges::default()
            },
            tool_details: Vec::new(),
            navigation: None,
        })),
    }
}

#[test]
fn a_task_subscribe_reply_reaches_the_wire_before_the_tasks_notifications() {
    let (mut gateway, store) = gateway_with_project_context_and_store();
    store.write_task(&open_task("task-order")).unwrap();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());
    let gateway = SharedRpcGateway::new(gateway);

    gateway.dispatch_inbound(
        connection_id.clone(),
        request(
            "subscribe",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-order"),
                },
                known_version: None,
            },
        ),
        AppServerTime(10),
    );
    // Published after the subscription exists but before the reply is written.
    gateway.publish_committed_task_update(
        &committed_task_status_update(
            "task-order",
            2,
            openaide_app_server_protocol::snapshot::TaskStatus::Waiting,
        ),
        AppServerTime(11),
    );

    let order = gateway.transfer_outbound(&connection_id, |messages| {
        messages
            .into_iter()
            .map(|message| match message {
                OutboundMessage::Reply { id, .. } => format!("reply:{}", id.as_str().unwrap()),
                OutboundMessage::Event(_) => "event".to_string(),
                OutboundMessage::ServerRequest(_) => "server-request".to_string(),
                OutboundMessage::Sync(_) => "sync".to_string(),
            })
            .collect::<Vec<_>>()
    });
    let reply = order.iter().position(|entry| entry == "reply:subscribe");
    let first_event = order.iter().position(|entry| entry == "event");
    assert!(reply.is_some() && first_event.is_some(), "{order:?}");
    assert!(reply < first_event, "{order:?}");
}

#[test]
fn a_deleted_task_ends_its_subscriptions_with_a_final_removed_notification() {
    let (mut gateway, store) = gateway_with_project_context_and_store();
    store.write_task(&open_task("task-removed")).unwrap();
    initialize_client(&mut gateway, ConnectionId::new("conn-1"));
    assert_eq!(
        subscribe(&mut gateway, "task-removed", None)["snapshot"]["kind"],
        "task"
    );
    let scope = SubscriptionScope::Task {
        task_id: TaskId::from("task-removed"),
    };
    assert_eq!(gateway.state_stream.subscribers_for_scope(&scope).len(), 1);

    let removal =
        gateway.publish_task_update(&removed_update("task-removed", 2), AppServerTime(11));

    let removed_events = removal
        .iter()
        .filter(|delivery| {
            matches!(
                &delivery.event.payload,
                AppServerEventPayload::TaskChanged { changes, .. } if changes.removed
            )
        })
        .count();
    assert_eq!(removed_events, 1);
    assert!(gateway
        .state_stream
        .subscribers_for_scope(&scope)
        .is_empty());
    let later = gateway.publish_task_update(
        &committed_task_status_update(
            "task-removed",
            3,
            openaide_app_server_protocol::snapshot::TaskStatus::Idle,
        ),
        AppServerTime(12),
    );
    assert!(later.is_empty(), "no notification follows the final one");
}

#[test]
fn a_known_version_subscribe_answers_current_without_chat() {
    let (mut gateway, store) = gateway_with_project_context_and_store();
    store.write_task(&open_task("task-current")).unwrap();
    initialize_client(&mut gateway, ConnectionId::new("conn-1"));
    let baseline = subscribe(&mut gateway, "task-current", None);
    let version = baseline["snapshot"]["chatVersion"].as_u64().unwrap();

    let resumed = subscribe(&mut gateway, "task-current", Some(version));

    assert_eq!(resumed["snapshot"]["kind"], "task");
    assert_eq!(resumed["snapshot"]["chatVersion"], json!(version));
    assert_eq!(resumed["snapshot"]["chatSync"]["kind"], "current");
}
