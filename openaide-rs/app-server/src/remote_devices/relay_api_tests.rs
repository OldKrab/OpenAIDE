use super::*;

fn error(status: u16, code: &str) -> RelayApiError {
    classify(status, format!(r#"{{"error":"{code}"}}"#).into_bytes()).unwrap_err()
}

#[test]
fn a_success_passes_the_body_through() {
    assert_eq!(classify(200, b"{}".to_vec()), Ok(b"{}".to_vec()));
}

#[test]
fn the_relay_error_codes_are_classified() {
    assert_eq!(error(401, "unknown_key"), RelayApiError::Refused);
    assert_eq!(error(403, "cannot_manage"), RelayApiError::CannotManage);
    assert_eq!(error(400, "invalid_name"), RelayApiError::InvalidName);
    assert_eq!(error(409, "name_taken"), RelayApiError::NameTaken);
    assert_eq!(error(404, "no_such_person"), RelayApiError::NoSuchPerson);
    assert_eq!(error(409, "last_manager"), RelayApiError::LastManager);
    assert_eq!(error(502, ""), RelayApiError::Unreachable);
}

#[test]
fn a_relay_without_the_api_is_told_apart_from_a_missing_person() {
    assert_eq!(
        classify(404, b"Not Found".to_vec()).unwrap_err(),
        RelayApiError::NoManagement
    );
}
