use std::sync::mpsc;
use std::sync::Arc;

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::methods::TASK_ATTENTION_WAIT;
use openaide_app_server_protocol::snapshot::TaskStatus;
use serde_json::{json, Value};

use super::AttentionWaitTiming;
use crate::protocol_edge::local_http::LocalHttpProtocolHandler;
use crate::protocol_edge::task_attention_handlers::tests::{task, OpenTasks};
use crate::protocol_edge::SharedRpcGateway;
use crate::test_sync::{EXPIRES, NEVER, WATCHDOG};

struct Fixture {
    tasks: Arc<OpenTasks>,
    gateway: SharedRpcGateway,
    handler: LocalHttpProtocolHandler,
}

/// An initialized `client-1` whose held waits end only as `deadline` says.
fn fixture(deadline: std::time::Duration) -> Fixture {
    let tasks = OpenTasks::new(vec![task("task-1", Some("event-1"), TaskStatus::Idle)]);
    let mut gateway =
        crate::protocol_edge::tests::initialized_gateway("client-1", "local-http:client-1");
    gateway.task_snapshots = tasks.clone();
    let gateway = SharedRpcGateway::new(gateway);
    let handler = LocalHttpProtocolHandler::new(gateway.clone(), "token", "server-1")
        .with_attention_wait(AttentionWaitTiming {
            deadline,
            // Only an explicit delivery wake may end a hold in these tests.
            fallback_wake: NEVER,
            coalesce: std::time::Duration::ZERO, // timing: data — no burst to coalesce
        });
    Fixture {
        tasks,
        gateway,
        handler,
    }
}

fn wait(handler: &LocalHttpProtocolHandler, connection_id: &str, cursor: Option<&str>) -> Value {
    let params = cursor.map_or_else(|| json!({}), |cursor| json!({ "cursor": cursor }));
    let body = json!({
        "jsonrpc": "2.0",
        "id": "wait-1",
        "method": TASK_ATTENTION_WAIT,
        "params": params,
        "meta": RequestMeta::default(),
    })
    .to_string();
    let response = handler.handle(Some("Bearer token"), Some(connection_id), &body);
    assert_eq!(response.status, 200);
    let messages: Value = serde_json::from_str(&response.body).unwrap();
    messages
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["id"] == "wait-1")
        .cloned()
        .unwrap()
}

fn cursor(message: &Value) -> String {
    message["result"]["result"]["cursor"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn first_wait_answers_at_once_with_current_attention() {
    let fixture = fixture(NEVER);

    let answer = wait(&fixture.handler, "client-1", None);

    let tasks = &answer["result"]["result"]["tasks"];
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    assert_eq!(tasks[0]["attention"]["eventId"], "event-1");
}

#[test]
fn held_wait_answers_once_the_attention_set_changes() {
    let fixture = fixture(NEVER);
    let held = cursor(&wait(&fixture.handler, "client-1", None));
    let (sender, answers) = mpsc::channel();
    let handler = fixture.handler.clone();
    let request_cursor = held.clone();
    std::thread::spawn(move || {
        let _ = sender.send(wait(&handler, "client-1", Some(&request_cursor)));
    });

    // A change read before or after the hold starts ends it the same way.
    fixture
        .tasks
        .replace(vec![task("task-1", Some("event-2"), TaskStatus::Idle)]);
    fixture.gateway.delivery_signal().notify();

    let answer = answers.recv_timeout(WATCHDOG).expect("held wait answered");
    assert_ne!(cursor(&answer), held);
    assert_eq!(
        answer["result"]["result"]["tasks"][0]["attention"]["eventId"],
        "event-2"
    );
}

#[test]
fn held_wait_ends_at_its_deadline_with_the_same_cursor() {
    // timing: expiry — nothing changes the Tasks, so only the deadline can end the hold.
    let fixture = fixture(EXPIRES);
    let held = cursor(&wait(&fixture.handler, "client-1", None));

    let answer = wait(&fixture.handler, "client-1", Some(&held));

    assert_eq!(cursor(&answer), held);
}

#[test]
fn wait_from_an_uninitialized_connection_is_answered_without_holding() {
    let fixture = fixture(NEVER);
    let held = cursor(&wait(&fixture.handler, "client-1", None));

    let answer = wait(&fixture.handler, "client-2", Some(&held));

    assert_eq!(answer["error"]["error"]["code"], "notInitialized");
}
