use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId, TaskId};
use openaide_app_server_protocol::snapshot::{PendingRequestKind, PendingRequestScope};
use serde_json::json;

use crate::client_lifecycle::{AppServerTime, ConnectionId, Delivery};

use super::*;

#[test]
fn opening_task_request_is_non_blocking_and_returns_deliveries() {
    let mut broker = ServerRequestBroker::new();

    let outcome = broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );

    let OpenRequestOutcome::Opened {
        snapshot,
        deliveries,
    } = outcome
    else {
        panic!("expected opened request");
    };
    assert_eq!(snapshot.title, "Permission needed");
    assert_eq!(snapshot.kind, PendingRequestKind::Permission);
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].envelope.method, "permission/request");
    assert_eq!(deliveries[0].envelope.params, json!({ "prompt": "Allow?" }));
}

#[test]
fn opening_deduplicates_deliveries_by_client_instance() {
    let mut broker = ServerRequestBroker::new();

    let outcome = broker.open(
        task_request("task-1"),
        vec![
            delivery("client-1", "conn-1"),
            delivery("client-1", "conn-2"),
        ],
        AppServerTime(1),
    );

    let OpenRequestOutcome::Opened { deliveries, .. } = outcome else {
        panic!("expected opened request");
    };
    assert_eq!(deliveries.len(), 1);
    assert_eq!(
        deliveries[0].delivery.client_instance_id,
        ClientInstanceId::from("client-1")
    );
}

#[test]
fn client_scoped_request_fails_when_target_disconnects() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        client_request("client-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );
    let request_id = opened_request_id(opened);

    let outcomes =
        broker.observe_responder_unavailable(&ClientInstanceId::from("client-1"), AppServerTime(2));

    assert_eq!(
        outcomes,
        vec![RequestLifecycleOutcome::Interrupted {
            request_id: request_id.clone(),
            scope: PendingRequestScope::Client {
                client_instance_id: ClientInstanceId::from("client-1")
            }
        }]
    );
    assert_eq!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id,
            ServerRequestAnswer::Result(json!({ "ok": true })),
            AppServerTime(3),
        ),
        ResponseOutcome::Interrupted {
            request_id: RequestId::from("server-request-1")
        }
    );
}

#[test]
fn task_scoped_request_survives_one_client_disconnect() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        task_request("task-1"),
        vec![
            delivery("client-1", "conn-1"),
            delivery("client-2", "conn-2"),
        ],
        AppServerTime(1),
    );
    let request_id = opened_request_id(opened);

    broker.observe_responder_unavailable(&ClientInstanceId::from("client-1"), AppServerTime(2));
    let outcome = broker.handle_response(
        ClientInstanceId::from("client-2"),
        request_id.clone(),
        ServerRequestAnswer::Result(json!({ "decision": "allow" })),
        AppServerTime(3),
    );

    assert!(matches!(
        outcome,
        ResponseOutcome::Accepted {
            request_id: accepted,
            responder,
            ..
        } if accepted == request_id && responder == ClientInstanceId::from("client-2")
    ));
}

#[test]
fn disconnected_task_responder_is_stale_until_redelivered() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );
    let request_id = opened_request_id(opened);

    broker.observe_responder_unavailable(&ClientInstanceId::from("client-1"), AppServerTime(2));

    assert_eq!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(3),
        ),
        ResponseOutcome::StaleRequest {
            request_id: request_id.clone(),
            responder: ClientInstanceId::from("client-1")
        }
    );

    let deliveries = broker.observe_responder_available(
        delivery("client-1", "conn-2"),
        &[ResponderScope::Task(TaskId::from("task-1"))],
        AppServerTime(4),
    );
    assert_eq!(deliveries.len(), 1);
    assert!(matches!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id,
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(5),
        ),
        ResponseOutcome::Accepted { .. }
    ));
}

#[test]
fn task_scoped_request_remains_pending_without_current_subscribers() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(task_request("task-1"), Vec::new(), AppServerTime(1));
    let request_id = opened_request_id(opened);

    assert_eq!(broker.pending_for_task(&TaskId::from("task-1")).len(), 1);
    let deliveries = broker.observe_subscription_added(
        delivery("client-1", "conn-1"),
        TaskId::from("task-1"),
        AppServerTime(2),
    );

    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].envelope.request_id, request_id);
}

#[test]
fn task_scoped_response_from_current_subscriber_is_accepted_before_delivery_drains() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(task_request("task-1"), Vec::new(), AppServerTime(1));
    let request_id = opened_request_id(opened);

    assert!(matches!(
        broker.handle_response_from_scopes(
            ClientInstanceId::from("client-1"),
            request_id,
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            &[ResponderScope::Task(TaskId::from("task-1"))],
            AppServerTime(2),
        ),
        ResponseOutcome::Accepted { .. }
    ));
}

#[test]
fn task_scoped_response_from_unsubscribed_client_is_rejected_before_delivery_drains() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(task_request("task-1"), Vec::new(), AppServerTime(1));
    let request_id = opened_request_id(opened);

    assert_eq!(
        broker.handle_response_from_scopes(
            ClientInstanceId::from("client-1"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            &[ResponderScope::Task(TaskId::from("task-2"))],
            AppServerTime(2),
        ),
        ResponseOutcome::UnauthorizedResponder {
            request_id,
            responder: ClientInstanceId::from("client-1"),
        }
    );
}

#[test]
fn responder_available_does_not_redeliver_existing_pending_request() {
    let mut broker = ServerRequestBroker::new();
    broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );

    let deliveries = broker.observe_responder_available(
        delivery("client-1", "conn-2"),
        &[ResponderScope::Task(TaskId::from("task-1"))],
        AppServerTime(2),
    );

    assert!(deliveries.is_empty());
}

#[test]
fn subscription_removed_stales_only_matching_task_request() {
    let mut broker = ServerRequestBroker::new();
    let task_1 = opened_request_id(broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    ));
    let task_2 = opened_request_id(broker.open(
        task_request("task-2"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    ));

    broker.observe_subscription_removed(
        &ClientInstanceId::from("client-1"),
        &TaskId::from("task-1"),
        AppServerTime(2),
    );

    assert_eq!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            task_1.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(3),
        ),
        ResponseOutcome::StaleRequest {
            request_id: task_1,
            responder: ClientInstanceId::from("client-1")
        }
    );
    assert!(matches!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            task_2,
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(4),
        ),
        ResponseOutcome::Accepted { .. }
    ));
}

#[test]
fn capability_unavailable_stales_matching_responder_until_available() {
    let mut broker = ServerRequestBroker::new();
    let request_id = opened_request_id(broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    ));

    broker.observe_capability_unavailable(
        &ClientInstanceId::from("client-1"),
        &[ResponderScope::Task(TaskId::from("task-1"))],
        AppServerTime(2),
    );
    assert!(matches!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(3),
        ),
        ResponseOutcome::StaleRequest { .. }
    ));

    let deliveries = broker.observe_capability_available(
        delivery("client-1", "conn-2"),
        &[ResponderScope::Task(TaskId::from("task-1"))],
        AppServerTime(4),
    );
    assert_eq!(deliveries.len(), 1);
    assert!(matches!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id,
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(5),
        ),
        ResponseOutcome::Accepted { .. }
    ));
}

#[test]
fn first_valid_task_response_wins_and_late_response_is_stale() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        task_request("task-1"),
        vec![
            delivery("client-1", "conn-1"),
            delivery("client-2", "conn-2"),
        ],
        AppServerTime(1),
    );
    let request_id = opened_request_id(opened);

    assert!(matches!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(2),
        ),
        ResponseOutcome::Accepted { .. }
    ));
    assert_eq!(
        broker.handle_response(
            ClientInstanceId::from("client-2"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "deny" })),
            AppServerTime(3),
        ),
        ResponseOutcome::AlreadyResolved { request_id }
    );
}

#[test]
fn invalid_response_does_not_resolve_request() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        task_request("task-1"),
        vec![
            delivery("client-1", "conn-1"),
            delivery("client-2", "conn-2"),
        ],
        AppServerTime(1),
    );
    let request_id = opened_request_id(opened);

    assert!(matches!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id.clone(),
            ServerRequestAnswer::Invalid("missing decision".to_string()),
            AppServerTime(2),
        ),
        ResponseOutcome::InvalidResponse { .. }
    ));
    assert!(matches!(
        broker.handle_response(
            ClientInstanceId::from("client-2"),
            request_id,
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(3),
        ),
        ResponseOutcome::Accepted { .. }
    ));
}

#[test]
fn unauthorized_client_cannot_answer() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );
    let request_id = opened_request_id(opened);

    assert_eq!(
        broker.handle_response(
            ClientInstanceId::from("client-2"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(2),
        ),
        ResponseOutcome::UnauthorizedResponder {
            request_id,
            responder: ClientInstanceId::from("client-2")
        }
    );
}

#[test]
fn unauthorized_client_cannot_observe_resolved_request_state() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );
    let request_id = opened_request_id(opened);

    assert!(matches!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(2),
        ),
        ResponseOutcome::Accepted { .. }
    ));
    assert_eq!(
        broker.handle_response(
            ClientInstanceId::from("client-2"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "deny" })),
            AppServerTime(3),
        ),
        ResponseOutcome::UnauthorizedResponder {
            request_id,
            responder: ClientInstanceId::from("client-2")
        }
    );
}

#[test]
fn interruption_prevents_later_mutation() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );
    let request_id = opened_request_id(opened);
    broker.interrupt_scope(
        &PendingRequestScope::Task {
            task_id: TaskId::from("task-1"),
        },
        AppServerTime(2),
    );

    assert_eq!(
        broker.handle_response(
            ClientInstanceId::from("client-1"),
            request_id.clone(),
            ServerRequestAnswer::Result(json!({ "decision": "allow" })),
            AppServerTime(3),
        ),
        ResponseOutcome::Interrupted { request_id }
    );
    assert!(broker.pending_for_task(&TaskId::from("task-1")).is_empty());
}

#[test]
fn snapshot_projection_contains_safe_pending_rows_only() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        ServerRequestDraft {
            scope: PendingRequestScope::Client {
                client_instance_id: ClientInstanceId::from("client-1"),
            },
            method: "secret/read".to_string(),
            title: "Secret needed".to_string(),
            params: json!({ "rawPath": "/private/path", "secretName": "TOKEN" }),
        },
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );
    opened_request_id(opened);

    let rows = broker.pending_for_client(&ClientInstanceId::from("client-1"));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Secret needed");
    let row_value = serde_json::to_value(&rows[0]).unwrap();
    assert!(row_value.get("rawPath").is_none());
    assert!(row_value.get("secretName").is_none());
}

#[test]
fn permission_snapshot_projects_answerable_safe_payload() {
    let mut broker = ServerRequestBroker::new();
    let opened = broker.open(
        ServerRequestDraft {
            scope: PendingRequestScope::Task {
                task_id: TaskId::from("task-1"),
            },
            method: "permission/request".to_string(),
            title: "Allow command?".to_string(),
            params: json!({
                "title": "Allow command?",
                "description": "Run command",
                "scope": "workspace",
                "risk": "writes files",
                "toolCall": { "id": "tool-1", "title": "Shell command", "kind": "execute" },
                "options": [
                    { "optionId": "allow-once", "name": "Allow", "kind": "allowOnce" },
                    { "optionId": "reject-once", "name": "Deny", "kind": "rejectOnce" }
                ],
                "rawPath": "/private/path"
            }),
        },
        Vec::new(),
        AppServerTime(1),
    );
    opened_request_id(opened);

    let rows = broker.pending_for_task(&TaskId::from("task-1"));
    assert_eq!(rows.len(), 1);
    let row_value = serde_json::to_value(&rows[0]).unwrap();
    assert_eq!(row_value["permission"]["title"], "Allow command?");
    assert_eq!(row_value["permission"]["toolCall"]["id"], "tool-1");
    assert_eq!(
        row_value["permission"]["options"][0]["optionId"],
        "allow-once"
    );
    assert!(row_value.get("rawPath").is_none());
    assert!(row_value["permission"].get("rawPath").is_none());
}

#[test]
fn client_scoped_request_requires_current_target_delivery() {
    let mut broker = ServerRequestBroker::new();

    assert_eq!(
        broker.open(client_request("client-1"), Vec::new(), AppServerTime(1)),
        OpenRequestOutcome::Unavailable {
            reason: RequestUnavailableReason::NoEligibleResponder
        }
    );
    assert_eq!(
        broker.open(
            client_request("client-1"),
            vec![delivery("client-2", "conn-2")],
            AppServerTime(2),
        ),
        OpenRequestOutcome::Unavailable {
            reason: RequestUnavailableReason::NoEligibleResponder
        }
    );
}

#[test]
fn client_scoped_request_rejects_wrong_target_delivery() {
    let mut broker = ServerRequestBroker::new();

    assert_eq!(
        broker.open(
            client_request("client-1"),
            vec![delivery("client-2", "conn-2")],
            AppServerTime(1)
        ),
        OpenRequestOutcome::Unavailable {
            reason: RequestUnavailableReason::NoEligibleResponder
        }
    );
}

#[test]
fn repeated_responder_available_does_not_duplicate_delivery() {
    let mut broker = ServerRequestBroker::new();
    broker.open(
        task_request("task-1"),
        vec![delivery("client-1", "conn-1")],
        AppServerTime(1),
    );

    let deliveries = broker.observe_responder_available(
        delivery("client-1", "conn-1"),
        &[ResponderScope::Task(TaskId::from("task-1"))],
        AppServerTime(2),
    );

    assert!(deliveries.is_empty());
}

#[test]
fn unsupported_method_is_not_opened() {
    let mut broker = ServerRequestBroker::new();

    assert_eq!(
        broker.open(
            ServerRequestDraft {
                scope: PendingRequestScope::Task {
                    task_id: TaskId::from("task-1"),
                },
                method: "task/private".to_string(),
                title: "Unsupported".to_string(),
                params: json!({}),
            },
            vec![delivery("client-1", "conn-1")],
            AppServerTime(1),
        ),
        OpenRequestOutcome::Unavailable {
            reason: RequestUnavailableReason::UnsupportedMethod
        }
    );
}

fn task_request(task_id: &str) -> ServerRequestDraft {
    ServerRequestDraft {
        scope: PendingRequestScope::Task {
            task_id: TaskId::from(task_id),
        },
        method: "permission/request".to_string(),
        title: "Permission needed".to_string(),
        params: json!({ "prompt": "Allow?" }),
    }
}

fn client_request(client_id: &str) -> ServerRequestDraft {
    ServerRequestDraft {
        scope: PendingRequestScope::Client {
            client_instance_id: ClientInstanceId::from(client_id),
        },
        method: "shell/revealFile".to_string(),
        title: "Reveal file".to_string(),
        params: json!({ "handleId": "handle-1" }),
    }
}

fn delivery(client_id: &str, connection_id: &str) -> Delivery {
    Delivery {
        client_instance_id: ClientInstanceId::from(client_id),
        connection_id: ConnectionId::new(connection_id),
    }
}

fn opened_request_id(outcome: OpenRequestOutcome) -> RequestId {
    match outcome {
        OpenRequestOutcome::Opened { snapshot, .. } => snapshot.request_id,
        other => panic!("expected opened request, got {other:?}"),
    }
}
