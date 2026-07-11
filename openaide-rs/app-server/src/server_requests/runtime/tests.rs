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
            Vec::new(),
            AppServerTime(1),
        )
        .expect("open permission request");
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
fn legacy_permission_response_after_protocol_answer_is_rejected_before_commit() {
    let runtime = ServerRequestRuntime::new();
    let agent_request = permission_request("agent-request-1");
    let request_id = runtime
        .open_permission_request(
            "task-1",
            &agent_request,
            vec![Delivery {
                client_instance_id: ClientInstanceId::from("client-1"),
                connection_id: ConnectionId::new("conn-1"),
            }],
            AppServerTime(1),
        )
        .expect("open permission request");
    runtime.handle_response(
        ClientInstanceId::from("client-1"),
        request_id,
        ServerRequestAnswer::Result(json!({ "optionId": "allow" })),
        AppServerTime(2),
    );

    let error = runtime
        .route_agent_permission_response(
            "agent-request-1",
            "deny".to_string(),
            |_| -> Result<(), crate::protocol::errors::RuntimeError> {
                panic!("legacy commit must not run after protocol answer")
            },
        )
        .unwrap_err();

    assert!(error.to_string().contains("permission already answered"));
}

#[test]
fn legacy_permission_response_clears_broker_pending_request() {
    let runtime = ServerRequestRuntime::new();
    let task_id = TaskId::from("task-1");
    let agent_request = permission_request("agent-request-1");
    runtime
        .open_permission_request("task-1", &agent_request, Vec::new(), AppServerTime(1))
        .expect("open permission request");

    runtime
        .route_agent_permission_response(
            "agent-request-1",
            "allow".to_string(),
            |_| -> Result<(), crate::protocol::errors::RuntimeError> { Ok(()) },
        )
        .expect("legacy permission response");

    assert!(runtime.pending_for_task(&task_id).is_empty());
    assert_eq!(runtime.pending_count(), 0);
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
