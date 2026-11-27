//! Subagent catalog, Subagent History and Tool detail subscriptions at the gateway (ADR-0086):
//! the reply precedes the subscription's notifications, one commit is one notification, and a
//! deleted Task ends each subscription with a final removal.

use openaide_app_server_protocol::events::{TaskChatChange, ToolDetailDelta};
use openaide_app_server_protocol::ids::SubagentId;
use openaide_app_server_protocol::snapshot::{SubagentCatalogSnapshot, TaskStatus};

use super::*;
use crate::protocol::model::NormalizedMessage;
use crate::protocol::model::{ActivityToolDetails, AgentMessagePart, AgentMessageRole};
use crate::protocol_edge::outbox::OutboundMessage;
use crate::storage::subagents::SubagentCapabilitiesRecord;
use crate::storage::task_journal::{TaskWrite, ToolArtifactReplacement, ToolTerminalAppend};

const TASK: &str = "task-entities";
const ARTIFACT: &str = "artifact_1";

struct Fixture {
    gateway: RpcGateway,
    store: Store,
    subagent: SubagentId,
}

fn fixture() -> Fixture {
    let (mut gateway, store) = gateway_with_project_context_and_store();
    let mut task = client_new_task_record(TASK, "client-1");
    task.lifecycle = crate::storage::records::TaskLifecycle::Open;
    store.write_task(&task).unwrap();
    store
        .task_journal()
        .submit(TaskWrite::barrier_operations_with_artifacts(
            TASK.to_string(),
            Vec::new(),
            vec![ToolArtifactReplacement {
                artifact_id: ARTIFACT.to_string(),
                details: ActivityToolDetails {
                    locations: Vec::new(),
                    content: Vec::new(),
                    input: None,
                    output: None,
                },
            }],
            vec![ToolTerminalAppend {
                artifact_id: ARTIFACT.to_string(),
                terminal_id: "terminal_1".to_string(),
                data: "one".to_string(),
            }],
        ))
        .unwrap()
        .wait()
        .unwrap();
    let subagent = SubagentId::from(
        store
            .record_subagent_spawn(
                TASK,
                "root-session",
                "child",
                "Researcher".to_string(),
                "Inspect".to_string(),
                SubagentCapabilitiesRecord::default(),
                Vec::new(),
            )
            .unwrap()
            .subagent_id,
    );
    initialize_client(&mut gateway, ConnectionId::new("conn-1"));
    Fixture {
        gateway,
        store,
        subagent,
    }
}

fn tool_scope() -> SubscriptionScope {
    SubscriptionScope::ToolDetail {
        task_id: TaskId::from(TASK),
        artifact_id: ARTIFACT.to_string(),
    }
}

fn history_scope(subagent: &SubagentId) -> SubscriptionScope {
    SubscriptionScope::SubagentHistory {
        task_id: TaskId::from(TASK),
        subagent_id: subagent.clone(),
    }
}

fn catalog_scope() -> SubscriptionScope {
    SubscriptionScope::SubagentCatalog {
        task_id: TaskId::from(TASK),
    }
}

fn subscribe(
    gateway: &mut RpcGateway,
    scope: SubscriptionScope,
    known_version: Option<u64>,
) -> serde_json::Value {
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "subscribe",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope,
                known_version,
            },
        ),
        AppServerTime(10),
    ))["result"]
        .clone()
}

fn history_changed(subagent: &SubagentId, revision: u64) -> TaskUpdate {
    use openaide_app_server_protocol::ids::MessageId;
    use openaide_app_server_protocol::snapshot::{ChatItem, ChatItemStatus, ChatRole};
    TaskUpdate {
        task_id: TASK.to_string(),
        revision,
        kind: TaskUpdateKind::SubagentHistoryChanged {
            subagent_id: subagent.clone(),
            chat: vec![TaskChatChange::Append {
                item: ChatItem {
                    message_id: MessageId::from("m1"),
                    turn_id: None,
                    role: ChatRole::Agent,
                    status: ChatItemStatus::Complete,
                    parts: Vec::new(),
                    timing: None,
                },
            }],
            current_plan: None,
        },
    }
}

fn catalog_changed(revision: u64) -> TaskUpdate {
    TaskUpdate {
        task_id: TASK.to_string(),
        revision,
        kind: TaskUpdateKind::SubagentCatalogChanged {
            catalog: SubagentCatalogSnapshot {
                task_id: TaskId::from(TASK),
                revision,
                entries: Vec::new(),
                has_more: false,
            },
        },
    }
}

fn tool_detail_commit(revision: u64) -> TaskUpdate {
    let mut update = committed_task_status_update(TASK, 5, TaskStatus::Running);
    let TaskUpdateKind::Changed(change) = &mut update.kind else {
        unreachable!("a status update is a committed change");
    };
    let mut details = fixed_tool_detail();
    details.revision = revision;
    change.tool_details = vec![ToolDetailUpdate {
        artifact_id: ARTIFACT.to_string(),
        details,
        terminal_appends: vec![
            crate::storage::task_journal::TerminalOutputAppend {
                terminal_id: "terminal_1".to_string(),
                data: "two".to_string(),
            },
            crate::storage::task_journal::TerminalOutputAppend {
                terminal_id: "terminal_2".to_string(),
                data: "x".to_string(),
            },
        ],
    }];
    update
}

fn removed_update() -> TaskUpdate {
    use openaide_app_server_protocol::events::TaskChanges;
    TaskUpdate {
        task_id: TASK.to_string(),
        revision: 9,
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

fn events_for<'a>(
    deliveries: &'a [GatewayEventDelivery],
    scope: &SubscriptionScope,
) -> Vec<&'a AppServerEventPayload> {
    deliveries
        .iter()
        .filter(|delivery| &delivery.event.subscription == scope)
        .map(|delivery| &delivery.event.payload)
        .collect()
}

#[test]
fn each_entity_reply_reaches_the_wire_before_its_notifications() {
    for (label, scope, update) in [
        ("tool detail", tool_scope(), tool_detail_commit(2)),
        ("catalog", catalog_scope(), catalog_changed(7)),
    ] {
        let Fixture { gateway, .. } = fixture();
        let connection_id = ConnectionId::new("conn-1");
        let gateway = SharedRpcGateway::new(gateway);
        gateway.dispatch_inbound(
            connection_id.clone(),
            request(
                "subscribe",
                STATE_SUBSCRIBE,
                StateSubscribeParams {
                    scope,
                    known_version: None,
                },
            ),
            AppServerTime(10),
        );
        gateway.publish_committed_task_update(&update, AppServerTime(11));
        assert_reply_precedes_events(&gateway, &connection_id, label);
    }
}

#[test]
fn a_subagent_history_reply_reaches_the_wire_before_its_notifications() {
    let Fixture {
        gateway, subagent, ..
    } = fixture();
    let connection_id = ConnectionId::new("conn-1");
    let gateway = SharedRpcGateway::new(gateway);
    gateway.dispatch_inbound(
        connection_id.clone(),
        request(
            "subscribe",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: history_scope(&subagent),
                known_version: None,
            },
        ),
        AppServerTime(10),
    );
    gateway.publish_committed_task_update(&history_changed(&subagent, 1), AppServerTime(11));

    assert_reply_precedes_events(&gateway, &connection_id, "subagent history");
}

fn assert_reply_precedes_events(
    gateway: &SharedRpcGateway,
    connection_id: &ConnectionId,
    label: &str,
) {
    let order = gateway.transfer_outbound(connection_id, |messages| {
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
    assert!(
        reply.is_some() && first_event.is_some(),
        "{label}: {order:?}"
    );
    assert!(reply < first_event, "{label}: {order:?}");
}

#[test]
fn a_tool_detail_commit_is_one_notification_with_fields_appends_and_version() {
    let Fixture { mut gateway, .. } = fixture();
    let baseline = subscribe(&mut gateway, tool_scope(), None);
    assert_eq!(baseline["snapshot"]["terminalSync"]["kind"], "full");
    assert_eq!(baseline["snapshot"]["details"]["revision"], json!(1));

    let published = gateway.publish_task_update(&tool_detail_commit(2), AppServerTime(11));

    let events = events_for(&published, &tool_scope());
    assert_eq!(events.len(), 1, "{events:?}");
    let AppServerEventPayload::ToolDetailChanged {
        revision, deltas, ..
    } = events[0]
    else {
        panic!("expected a tool detail change, got {:?}", events[0]);
    };
    assert_eq!(*revision, 2);
    assert!(matches!(&deltas[0], ToolDetailDelta::ReplaceDetails { .. }));
    assert_eq!(
        deltas[1..]
            .iter()
            .map(|delta| match delta {
                ToolDetailDelta::AppendTerminal { terminal_id, data } => {
                    (terminal_id.as_str(), data.as_str())
                }
                other => panic!("unexpected delta {other:?}"),
            })
            .collect::<Vec<_>>(),
        [("terminal_1", "two"), ("terminal_2", "x")]
    );
}

#[test]
fn a_known_tool_detail_version_resumes_over_the_gateway() {
    let Fixture {
        mut gateway, store, ..
    } = fixture();
    store
        .task_journal()
        .submit(TaskWrite::stream_append_terminal(
            TASK,
            ARTIFACT,
            "terminal_1",
            "two",
        ))
        .unwrap()
        .wait()
        .unwrap();

    let current = subscribe(&mut gateway, tool_scope(), Some(2));
    let deltas = subscribe(&mut gateway, tool_scope(), Some(1));
    let full = subscribe(&mut gateway, tool_scope(), None);

    assert_eq!(current["snapshot"]["terminalSync"]["kind"], "current");
    assert_eq!(deltas["snapshot"]["terminalSync"]["kind"], "deltas");
    assert_eq!(
        deltas["snapshot"]["terminalSync"]["appends"],
        json!([{ "terminalId": "terminal_1", "data": "two" }])
    );
    assert_eq!(full["snapshot"]["terminalSync"]["kind"], "full");
    assert_eq!(
        full["snapshot"]["details"]["terminalOutputs"],
        json!([{ "terminalId": "terminal_1", "output": "onetwo" }])
    );
}

#[test]
fn a_known_subagent_history_version_resumes_over_the_gateway() {
    let Fixture {
        mut gateway,
        store,
        subagent,
    } = fixture();
    store
        .append_subagent_message(
            TASK,
            "child",
            NormalizedMessage::AgentMessage {
                id: "m1".to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: "hello".to_string(),
                }],
                created_at: "1".to_string(),
            },
            false,
        )
        .unwrap()
        .wait()
        .unwrap();

    let full = subscribe(&mut gateway, history_scope(&subagent), None);
    let current = subscribe(&mut gateway, history_scope(&subagent), Some(1));
    let stale = subscribe(&mut gateway, history_scope(&subagent), Some(0));

    assert_eq!(full["snapshot"]["chatSync"]["kind"], "full");
    assert_eq!(full["snapshot"]["history"]["revision"], json!(1));
    assert_eq!(current["snapshot"]["chatSync"]["kind"], "current");
    assert_eq!(stale["snapshot"]["chatSync"]["kind"], "full");
}

#[test]
fn a_subagent_history_commit_is_one_notification_with_its_version() {
    let Fixture {
        mut gateway,
        subagent,
        ..
    } = fixture();
    subscribe(&mut gateway, history_scope(&subagent), None);

    let published = gateway.publish_task_update(&history_changed(&subagent, 1), AppServerTime(11));

    let events = events_for(&published, &history_scope(&subagent));
    assert_eq!(events.len(), 1);
    let AppServerEventPayload::SubagentHistoryChanged { revision, chat, .. } = events[0] else {
        panic!("expected a history change, got {:?}", events[0]);
    };
    assert_eq!(*revision, 1);
    assert_eq!(chat.len(), 1);
}

#[test]
fn a_deleted_task_ends_each_entity_subscription_with_a_final_removal() {
    let Fixture {
        mut gateway,
        subagent,
        ..
    } = fixture();
    let scopes = [tool_scope(), history_scope(&subagent), catalog_scope()];
    for scope in &scopes {
        subscribe(&mut gateway, scope.clone(), None);
        assert_eq!(gateway.state_stream.subscribers_for_scope(scope).len(), 1);
    }

    let removal = gateway.publish_task_update(&removed_update(), AppServerTime(11));

    for scope in &scopes {
        let events = events_for(&removal, scope);
        assert_eq!(events.len(), 1, "{scope:?}: {events:?}");
        assert!(
            matches!(events[0], AppServerEventPayload::TaskContentRemoved { .. }),
            "{scope:?}: {events:?}"
        );
        assert!(gateway.state_stream.subscribers_for_scope(scope).is_empty());
    }
    let later = gateway.publish_task_update(&history_changed(&subagent, 2), AppServerTime(12));
    assert!(events_for(&later, &history_scope(&subagent)).is_empty());
}

#[test]
fn another_tasks_changes_do_not_reach_an_entity_subscription() {
    let Fixture { mut gateway, .. } = fixture();
    subscribe(&mut gateway, tool_scope(), None);
    let mut other = tool_detail_commit(2);
    other.task_id = "task-elsewhere".to_string();

    let published = gateway.publish_task_update(&other, AppServerTime(11));

    assert!(events_for(&published, &tool_scope()).is_empty());
}
