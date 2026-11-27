use super::*;

#[test]
fn an_unregistered_method_has_no_handler() {
    assert!(client_notification_handler("client/unknown").is_none());
}

fn rejections(logs: &[Value]) -> Vec<Value> {
    logs.iter()
        .filter(|line| line["event"] == json!("client_notification_rejected"))
        .map(|line| line["fields"]["reason_code"].clone())
        .collect()
}

#[test]
fn an_unanswerable_notification_is_logged_and_changes_nothing() {
    let logs = crate::logging::capture_test_logs();
    let mut gateway = super::super::tests::initialized_gateway("client-1", "conn-1");

    let unknown = gateway.handle_client_notification(
        ConnectionId::new("conn-1"),
        "client/unknown".to_string(),
        json!({}),
        AppServerTime(2),
    );
    let uninitialized = gateway.handle_client_notification(
        ConnectionId::new("conn-2"),
        "client/unknown".to_string(),
        json!({}),
        AppServerTime(2),
    );

    assert_eq!(unknown, GatewayOutcome::Noop);
    assert_eq!(uninitialized, GatewayOutcome::Noop);
    assert_eq!(
        rejections(&logs.snapshot()),
        [json!("unknown_method"), json!("not_initialized")]
    );
}
