use super::*;

#[test]
fn acp_error_reports_authentication_required_as_auth_required() {
    let error = acp_error("Authentication required: { \"data\": null }");

    assert_eq!(
        error.to_string(),
        "agent authentication required: Authentication required. Open Settings and authenticate this Agent before starting a Task."
    );
}

#[test]
fn acp_request_error_reports_missing_codex_rollout_as_task_not_found() {
    let error = agent_client_protocol::util::internal_error(
        r#"{"details":"no rollout found for thread id native-session-1"}"#,
    );

    let normalized = acp_request_error(&error);

    assert!(matches!(normalized, RuntimeError::TaskNotFound(_)));
}
