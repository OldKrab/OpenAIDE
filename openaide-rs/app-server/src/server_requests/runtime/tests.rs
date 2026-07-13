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
use std::time::Duration;

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
fn waitable_client_request_returns_accepted_response() {
    let runtime = ServerRequestRuntime::new();
    let opened = runtime
        .open_secret_read_request(
            ClientInstanceId::from("client-1"),
            Delivery {
                client_instance_id: ClientInstanceId::from("client-1"),
                connection_id: ConnectionId::new("conn-1"),
                request_capabilities: Vec::new(),
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
        .wait_client_response(&opened.request_id, Duration::from_secs(1))
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
                request_capabilities: Vec::new(),
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
                request_capabilities: Vec::new(),
            },
            ShellNotificationLevel::Info,
            "Saved".to_string(),
            Vec::new(),
            AppServerTime(1),
        )
        .expect("open request");

    let error = runtime
        .wait_client_response(&opened.request_id, Duration::from_millis(1))
        .unwrap_err();

    assert!(error.to_string().contains("timed out"));
    assert!(runtime
        .pending_for_client(&ClientInstanceId::from("client-1"))
        .is_empty());
}

#[test]
fn waitable_task_request_delivers_when_task_responder_subscribes() {
    let runtime = ServerRequestRuntime::new();
    let task_id = TaskId::from("task-1");
    let opened = runtime
        .open_task_secret_read_request(
            task_id.clone(),
            "agent.secret".to_string(),
            Some("Agent secret".to_string()),
            AppServerTime(1),
        )
        .expect("open task request");

    assert!(opened.deliveries.is_empty());
    assert_eq!(runtime.pending_for_task(&task_id).len(), 1);

    let deliveries = runtime.observe_subscription_added(
        Delivery {
            client_instance_id: ClientInstanceId::from("client-1"),
            connection_id: ConnectionId::new("conn-1"),
            request_capabilities: Vec::new(),
        },
        task_id,
        AppServerTime(2),
    );

    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].envelope.request_id, opened.request_id);
    assert_eq!(deliveries[0].envelope.method, "secret/read");
    runtime.handle_response(
        ClientInstanceId::from("client-1"),
        opened.request_id.clone(),
        ServerRequestAnswer::Result(json!({ "value": "secret-value" })),
        AppServerTime(3),
    );

    let result = runtime
        .wait_client_response(&opened.request_id, Duration::from_secs(1))
        .expect("accepted response");
    assert_eq!(result, json!({ "value": "secret-value" }));
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
    .with_request_capabilities(vec![crate::client_lifecycle::RequestCapability::Permission])
}
