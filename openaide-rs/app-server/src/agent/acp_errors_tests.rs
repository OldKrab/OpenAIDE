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

    assert!(matches!(normalized, RuntimeError::NativeSessionMissing(_)));
}

#[test]
fn acp_request_error_reports_an_active_codex_writer_as_conflict() {
    let error = agent_client_protocol::util::internal_error(
        r#"{"details":"thread native-session-1 already has an active writer"}"#,
    );

    let normalized = acp_request_error(&error);

    assert!(matches!(
        normalized,
        RuntimeError::Conflict(message)
            if message == "Native Session is currently in use elsewhere"
    ));
}

#[test]
fn missing_files_and_unloaded_threads_do_not_prove_missing_session_history() {
    for message in [
        "configuration file not found",
        "workspace does not exist",
        "thread not loaded: native-1",
    ] {
        let error = agent_client_protocol::util::internal_error(message);
        assert_ne!(acp_request_error(&error).reason(), "native_session_missing");
    }
    let error =
        agent_client_protocol::util::internal_error("no rollout found for thread id native-1");
    assert_eq!(acp_request_error(&error).reason(), "native_session_missing");
}

#[test]
fn prompt_sign_in_refusal_is_recognised_by_code_whatever_the_wording() {
    let error = agent_client_protocol::Error::auth_required();
    let reworded =
        agent_client_protocol::Error::new(i32::from(error.code), "401 Unauthorized: token revoked");

    assert!(matches!(
        acp_prompt_error(&reworded),
        RuntimeError::AuthRequired(_)
    ));
}

#[test]
fn prompt_failure_carries_the_agents_own_explanation() {
    let in_message = agent_client_protocol::Error::internal_error();
    let in_message = agent_client_protocol::Error::new(
        i32::from(in_message.code),
        "You have reached your  usage limit.\nTry again at 5pm.",
    );
    let in_data = agent_client_protocol::Error::internal_error()
        .data(serde_json::json!({ "message": "Rate limit reached", "codexErrorInfo": "x" }));

    assert!(matches!(
        acp_prompt_error(&in_message),
        RuntimeError::AgentReported(message)
            if message == "You have reached your usage limit. Try again at 5pm."
    ));
    assert!(matches!(
        acp_prompt_error(&in_data),
        RuntimeError::AgentReported(message) if message == "Rate limit reached"
    ));
}

#[test]
fn prompt_failure_without_an_explanation_stays_a_diagnostic() {
    let bare = agent_client_protocol::Error::internal_error();
    let kind_only = agent_client_protocol::Error::internal_error()
        .data(serde_json::json!({ "errorKind": "no_result" }));

    assert!(matches!(acp_prompt_error(&bare), RuntimeError::Internal(_)));
    assert!(matches!(
        acp_prompt_error(&kind_only),
        RuntimeError::Internal(_)
    ));
}

#[test]
fn a_long_agent_explanation_is_cut_for_chat() {
    let code = i32::from(agent_client_protocol::Error::internal_error().code);
    let error = agent_client_protocol::Error::new(code, "é".repeat(2000));

    let RuntimeError::AgentReported(message) = acp_prompt_error(&error) else {
        panic!("expected the Agent's explanation");
    };
    assert_eq!(message.chars().count(), 601);
    assert!(message.ends_with('…'));
}
