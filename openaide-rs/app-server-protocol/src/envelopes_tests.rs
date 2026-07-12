use serde_json::json;

use super::*;
use crate::client::ShellKind;
use crate::client::{ClientCapabilities, InitializeParams, RequestedSurface, ShellDescriptor};
use crate::errors::ProtocolErrorCode;
use crate::methods::{ClientInitialize, ProtocolMethod};
use crate::snapshot::PendingRequestScope;

#[test]
fn client_request_envelope_carries_method_params_and_meta() {
    let request = ClientInitialize::request(
        InitializeParams {
            client_instance_id: "client-1".into(),
            shell: ShellDescriptor {
                kind: ShellKind::Web,
                name: None,
                version: None,
            },
            requested_surface: RequestedSurface::Home,
            capabilities: ClientCapabilities::default(),
        },
        RequestMeta {
            client_request_id: Some("request-1".into()),
        },
    );

    let value = serde_json::to_value(request).unwrap();

    assert_eq!(value["method"], json!("client/initialize"));
    assert_eq!(value["params"]["clientInstanceId"], json!("client-1"));
    assert_eq!(value["params"]["requestedSurface"]["kind"], json!("home"));
    assert_eq!(value["meta"]["clientRequestId"], json!("request-1"));
}

#[test]
fn response_and_error_envelopes_carry_client_request_id() {
    let response = ResponseEnvelope::new(
        "accepted",
        ResponseMeta {
            client_request_id: Some("request-1".into()),
        },
    );
    let error = ErrorEnvelope::new(
        ProtocolError {
            code: ProtocolErrorCode::ValidationFailed,
            message: "invalid".to_string(),
            recoverable: true,
            target: None,
        },
        ResponseMeta {
            client_request_id: Some("request-1".into()),
        },
    );

    let response_value = serde_json::to_value(response).unwrap();
    let error_value = serde_json::to_value(error).unwrap();

    assert_eq!(response_value["result"], json!("accepted"));
    assert_eq!(
        response_value["meta"]["clientRequestId"],
        json!("request-1")
    );
    assert_eq!(error_value["error"]["code"], json!("validationFailed"));
    assert_eq!(error_value["meta"]["clientRequestId"], json!("request-1"));
}

#[test]
fn server_request_envelope_carries_stable_request_id() {
    let request = ServerRequestEnvelope::new(
        "request-1".into(),
        PendingRequestScope::Task {
            task_id: "task-1".into(),
        },
        "shell/showNotification",
        (),
    );
    let value = serde_json::to_value(request).unwrap();

    assert_eq!(value["requestId"], json!("request-1"));
    assert_eq!(value["scope"]["kind"], json!("task"));
    assert_eq!(value["scope"]["taskId"], json!("task-1"));
    assert_eq!(value["method"], json!("shell/showNotification"));
}
