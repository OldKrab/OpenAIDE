use crate::agent::events::{
    AgentPermissionOption, AgentPermissionOptionKind, AgentPermissionOutcome,
    AgentPermissionRequest, AgentToolCallRef,
};
use crate::agent::TurnCancellation;
use crate::client_lifecycle::{AppServerTime, ConnectionId, Delivery};
use crate::server_requests::ServerRequestAnswer;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::server_requests::ShellNotificationLevel;
use serde_json::json;

use super::ServerRequestRuntime;

#[test]
fn cancelled_permission_wait_clears_broker_pending_request() {
    let runtime = ServerRequestRuntime::new();
    let request_id = runtime
        .open_permission_request(
            "task-1",
            &permission_request("agent-request-1"),
            vec![permission_delivery()],
            AppServerTime(1),
        )
        .expect("open permission request")
        .expect("capable permission responder");
    assert_eq!(
        runtime
            .pending_for_task(&openaide_app_server_protocol::ids::TaskId::from("task-1"))
            .len(),
        1
    );

    let cancellation = TurnCancellation::new();
    cancellation.cancel();
    let response = runtime
        .wait_permission_response(&request_id, &cancellation)
        .expect("wait cancelled");

    assert!(matches!(
        response.outcome,
        AgentPermissionOutcome::Cancelled
    ));
    assert!(runtime
        .pending_for_task(&openaide_app_server_protocol::ids::TaskId::from("task-1"))
        .is_empty());
}

#[test]
fn interrupting_task_requests_releases_permission_wait_as_cancelled() {
    let runtime = ServerRequestRuntime::new();
    let task_id = TaskId::from("task-1");
    let request_id = runtime
        .open_permission_request(
            task_id.as_str(),
            &permission_request("agent-request-1"),
            vec![permission_delivery()],
            AppServerTime(1),
        )
        .expect("open permission request")
        .expect("capable permission responder");

    assert_eq!(
        runtime.interrupt_task_requests(&task_id, AppServerTime(2)),
        1
    );
    let response = runtime
        .wait_permission_response(&request_id, &TurnCancellation::new())
        .expect("interrupted wait settles");

    assert!(matches!(
        response.outcome,
        AgentPermissionOutcome::Cancelled
    ));
    assert!(runtime.pending_for_task(&task_id).is_empty());
}

#[test]
fn waitable_client_request_returns_accepted_response() {
    let runtime = ServerRequestRuntime::new();
    let opened = runtime
        .open_secret_read_request(
            ClientInstanceId::from("client-1"),
            Delivery {
                client_instance_id: ClientInstanceId::from("client-1"),
                connection_id: ConnectionId::new("conn-1"),
            },
            "agent.secret".to_string(),
            Some("Agent secret".to_string()),
            AppServerTime(1),
        )
        .expect("open request");

    assert_eq!(opened.deliveries.len(), 1);
    assert_eq!(opened.deliveries[0].envelope.method, "secret/read");

    runtime.handle_response(
        ClientInstanceId::from("client-1"),
        opened.request_id.clone(),
        ServerRequestAnswer::Result(json!({ "value": "secret-value" })),
        AppServerTime(2),
    );

    let result = runtime
        .wait_client_response(&opened.request_id, crate::test_sync::WATCHDOG)
        .expect("accepted response");
    assert_eq!(result, json!({ "value": "secret-value" }));
}

#[test]
fn shell_reveal_file_request_uses_opaque_handle_params() {
    let runtime = ServerRequestRuntime::new();
    let opened = runtime
        .open_shell_reveal_file_request(
            ClientInstanceId::from("client-1"),
            Delivery {
                client_instance_id: ClientInstanceId::from("client-1"),
                connection_id: ConnectionId::new("conn-1"),
            },
            "file-reveal-1".to_string(),
            Some("main.rs".to_string()),
            AppServerTime(1),
        )
        .expect("open reveal request");

    assert_eq!(opened.deliveries.len(), 1);
    let params = &opened.deliveries[0].envelope.params;
    assert_eq!(opened.deliveries[0].envelope.method, "shell/revealFile");
    assert_eq!(params["originatingClientInstanceId"], "client-1");
    assert_eq!(params["fileHandleId"], "file-reveal-1");
    assert_eq!(params["label"], "main.rs");
    assert!(params.get("path").is_none());
}

#[test]
fn waitable_client_request_timeout_interrupts_pending_request() {
    let runtime = ServerRequestRuntime::new();
    let opened = runtime
        .open_shell_notification_request(
            ClientInstanceId::from("client-1"),
            Delivery {
                client_instance_id: ClientInstanceId::from("client-1"),
                connection_id: ConnectionId::new("conn-1"),
            },
            ShellNotificationLevel::Info,
            "Saved".to_string(),
            Vec::new(),
            AppServerTime(1),
        )
        .expect("open request");

    let error = runtime
        .wait_client_response(&opened.request_id, crate::test_sync::EXPIRES)
        .unwrap_err();

    assert!(error.to_string().contains("timed out"));
    assert!(runtime
        .pending_for_client(&ClientInstanceId::from("client-1"))
        .is_empty());
}

fn holder_delivery(client: &str, connection: &str) -> Delivery {
    Delivery {
        client_instance_id: ClientInstanceId::from(client),
        connection_id: ConnectionId::new(connection),
    }
}

#[test]
fn task_secret_read_goes_to_the_connected_client_holding_the_key() {
    let runtime = ServerRequestRuntime::new();
    let task_id = TaskId::from("task-1");
    // Neither client subscribes to the Task: routing follows the declared keys alone.
    runtime.observe_responder_available(
        holder_delivery("client-without", "conn-0"),
        AppServerTime(1),
    );
    runtime.observe_responder_available(holder_delivery("client-1", "conn-1"), AppServerTime(1));
    runtime.declare_secret_keys(
        ClientInstanceId::from("client-1"),
        vec!["agent.secret".to_string()],
    );
    runtime.declare_secret_keys(
        ClientInstanceId::from("client-without"),
        vec!["other".to_string()],
    );

    let opened = runtime
        .open_task_secret_read_request(
            task_id,
            "agent.secret".to_string(),
            Some("Agent secret".to_string()),
            AppServerTime(2),
        )
        .expect("open task request");

    assert_eq!(opened.deliveries.len(), 1);
    assert_eq!(
        opened.deliveries[0].delivery.client_instance_id,
        ClientInstanceId::from("client-1")
    );
    assert_eq!(opened.deliveries[0].envelope.method, "secret/read");
    runtime.handle_response(
        ClientInstanceId::from("client-1"),
        opened.request_id.clone(),
        ServerRequestAnswer::Result(json!({ "value": "secret-value" })),
        AppServerTime(3),
    );
    let result = runtime
        .wait_client_response(&opened.request_id, crate::test_sync::WATCHDOG)
        .expect("accepted response");
    assert_eq!(result, json!({ "value": "secret-value" }));
}

#[test]
fn the_client_that_declared_a_key_last_is_asked_first() {
    let runtime = ServerRequestRuntime::new();
    for (client, connection) in [("client-1", "conn-1"), ("client-2", "conn-2")] {
        runtime.observe_responder_available(holder_delivery(client, connection), AppServerTime(1));
        runtime.declare_secret_keys(
            ClientInstanceId::from(client),
            vec!["agent.secret".to_string()],
        );
    }

    let opened = runtime
        .open_task_secret_read_request(
            TaskId::from("task-1"),
            "agent.secret".to_string(),
            None,
            AppServerTime(2),
        )
        .expect("open task request");

    assert_eq!(
        opened.deliveries[0].delivery.client_instance_id,
        ClientInstanceId::from("client-2")
    );
}

#[test]
fn an_unknown_holder_is_asked_only_when_no_client_declared_the_key() {
    let logs = crate::logging::capture_test_logs();
    let runtime = ServerRequestRuntime::new();
    runtime.observe_responder_available(holder_delivery("declared", "conn-1"), AppServerTime(1));
    runtime.observe_responder_available(holder_delivery("legacy", "conn-2"), AppServerTime(1));
    runtime.declare_secret_holding(ClientInstanceId::from("legacy"), Vec::new(), true);
    runtime.declare_secret_keys(
        ClientInstanceId::from("declared"),
        vec!["agent.secret".to_string()],
    );

    let declared = runtime
        .open_task_secret_read_request(
            TaskId::from("task-unknown-holder"),
            "agent.secret".to_string(),
            None,
            AppServerTime(2),
        )
        .expect("declared holder");
    // A key saved before the upgrade: nobody declared it, so only the unknown holder is asked.
    let legacy = runtime
        .open_task_secret_read_request(
            TaskId::from("task-unknown-holder"),
            "stored.before.upgrade".to_string(),
            None,
            AppServerTime(2),
        )
        .expect("unknown holder fallback");

    assert_eq!(
        declared.deliveries[0].delivery.client_instance_id,
        ClientInstanceId::from("declared")
    );
    assert_eq!(legacy.deliveries.len(), 1);
    assert_eq!(
        legacy.deliveries[0].delivery.client_instance_id,
        ClientInstanceId::from("legacy")
    );
    let fallbacks = logs
        .snapshot()
        .into_iter()
        .filter(|line| {
            line["event"] == json!("secret_read_routed")
                && line["fields"]["task_id"] == json!("task-unknown-holder")
        })
        .map(|line| line["fields"]["unknown_holder_fallback"].clone())
        .collect::<Vec<_>>();
    assert_eq!(fallbacks, vec![json!(false), json!(true)]);
}

#[test]
fn task_secret_read_with_no_connected_holder_fails_at_once_and_logs() {
    let logs = crate::logging::capture_test_logs();
    let runtime = ServerRequestRuntime::new();
    runtime.observe_responder_available(holder_delivery("client-1", "conn-1"), AppServerTime(1));
    runtime.declare_secret_keys(
        ClientInstanceId::from("client-1"),
        vec!["agent.secret".to_string()],
    );
    // The only holder loses its transport: a client in its reconnect grace cannot answer.
    runtime.observe_transport_unavailable(&ClientInstanceId::from("client-1"), AppServerTime(2));

    let started = std::time::Instant::now();
    let error = runtime
        .open_task_secret_read_request(
            TaskId::from("task-unrouted-log"),
            "agent.secret".to_string(),
            None,
            AppServerTime(3),
        )
        .expect_err("no connected holder");
    let unheld = runtime
        .open_task_secret_read_request(
            TaskId::from("task-unrouted-log"),
            "unheld".to_string(),
            None,
            AppServerTime(3),
        )
        .expect_err("nobody declared the key");

    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    for error in [error, unheld] {
        assert!(
            matches!(error, crate::protocol::errors::RuntimeError::NotReady(message) if message == "missing secure value")
        );
    }
    let unrouted: Vec<_> = logs
        .snapshot()
        .into_iter()
        .filter(|line| {
            line["event"] == json!("secret_read_unrouted")
                && line["fields"]["task_id"] == json!("task-unrouted-log")
        })
        .collect();
    assert_eq!(unrouted.len(), 2);
    assert_eq!(unrouted[0]["fields"]["holder_count"], json!(0));
    assert_eq!(unrouted[1]["fields"]["holder_count"], json!(0));
    assert!(!format!("{unrouted:?}").contains("agent.secret"));
}

fn permission_request(request_id: &str) -> AgentPermissionRequest {
    AgentPermissionRequest {
        request_id: request_id.to_string(),
        title: "Allow action?".to_string(),
        description: None,
        scope: None,
        risk: None,
        tool_call: AgentToolCallRef {
            tool_call_id: "tool-1".to_string(),
            title: "Tool".to_string(),
            kind: Some("edit".to_string()),
        },
        subagent_native_session_id: None,
        options: vec![
            AgentPermissionOption {
                option_id: "allow".to_string(),
                name: "Allow".to_string(),
                kind: AgentPermissionOptionKind::AllowOnce,
            },
            AgentPermissionOption {
                option_id: "deny".to_string(),
                name: "Deny".to_string(),
                kind: AgentPermissionOptionKind::RejectOnce,
            },
        ],
    }
}

fn permission_delivery() -> Delivery {
    Delivery::new(
        ClientInstanceId::from("client-1"),
        ConnectionId::new("conn-1"),
    )
}
