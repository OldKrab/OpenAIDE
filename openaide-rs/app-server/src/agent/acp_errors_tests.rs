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
