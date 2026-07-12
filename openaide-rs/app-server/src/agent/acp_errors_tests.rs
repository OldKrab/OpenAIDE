use super::*;

#[test]
fn acp_error_reports_authentication_required_as_auth_required() {
    let error = acp_error("Authentication required: { \"data\": null }");

    assert_eq!(
        error.to_string(),
        "agent authentication required: Authentication required. Open Settings and authenticate this Agent before starting a Task."
    );
}
