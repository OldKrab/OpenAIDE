use serde_json::json;

use super::*;

#[test]
fn permission_request_response_uses_protocol_camel_case() {
    let response = PermissionRequestResponse {
        option_id: "allow-once".to_string(),
    };

    let value = serde_json::to_value(response).unwrap();

    assert_eq!(value, json!({ "optionId": "allow-once" }));
}

#[test]
fn shell_and_secret_requests_use_protocol_safe_shapes() {
    let secret = SecretReadParams {
        key: "github.token".to_string(),
        label: Some("GitHub token".to_string()),
    };
    let notification = ShellShowNotificationParams {
        level: ShellNotificationLevel::Warning,
        message: "Credential required".to_string(),
        actions: vec![ShellNotificationAction {
            action_id: "open-settings".to_string(),
            label: "Open Settings".to_string(),
        }],
    };
    let reveal = ShellRevealFileParams {
        originating_client_instance_id: crate::ids::ClientInstanceId::from("client-1"),
        file_handle_id: "file-handle-1".to_string(),
        label: None,
    };

    assert_eq!(
        serde_json::to_value(secret).unwrap(),
        json!({ "key": "github.token", "label": "GitHub token" })
    );
    assert_eq!(
        serde_json::to_value(notification).unwrap(),
        json!({
            "level": "warning",
            "message": "Credential required",
            "actions": [{ "actionId": "open-settings", "label": "Open Settings" }]
        })
    );
    assert_eq!(
        serde_json::to_value(reveal).unwrap(),
        json!({
            "originatingClientInstanceId": "client-1",
            "fileHandleId": "file-handle-1"
        })
    );
}

#[test]
fn question_response_distinguishes_submit_from_cancel() {
    let response = QuestionRequestResponse::Submit {
        content: std::collections::BTreeMap::from([
            (
                "name".to_string(),
                QuestionValue::String("OpenAIDE".to_string()),
            ),
            ("enabled".to_string(), QuestionValue::Boolean(true)),
        ]),
    };

    assert_eq!(
        serde_json::to_value(response).unwrap(),
        json!({ "action": "submit", "content": { "enabled": true, "name": "OpenAIDE" } })
    );
    assert_eq!(
        serde_json::to_value(QuestionRequestResponse::Cancel).unwrap(),
        json!({ "action": "cancel" })
    );
}

#[test]
fn pending_request_resolution_is_a_typed_client_request_payload() {
    let permission = PendingRequestResolveParams {
        request_id: crate::ids::RequestId::from("request-1"),
        resolution: PendingRequestResolution::Permission {
            option_id: "allow-once".to_string(),
        },
    };

    assert_eq!(
        serde_json::to_value(permission).unwrap(),
        json!({
            "requestId": "request-1",
            "resolution": { "kind": "permission", "optionId": "allow-once" }
        })
    );
}
