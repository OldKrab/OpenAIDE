use std::time::Duration;

use super::*;

// timing: data — slow-wait thresholds compared against the measured wait, never waited on.
const ALWAYS_SLOW: Duration = Duration::ZERO;
const NEVER_SLOW: Duration = Duration::MAX; // timing: data

#[test]
fn client_request_reports_slow_protocol_lock_wait_with_method() {
    let logs = crate::logging::capture_test_logs();
    let connection_id = ConnectionId::new("conn-lock-wait-slow");
    let gateway = SharedRpcGateway::new(gateway()).with_slow_lock_wait(ALWAYS_SLOW);

    gateway.handle_inbound(
        connection_id.clone(),
        request("1", CLIENT_INITIALIZE, init_params("client-lock-wait-slow")),
        AppServerTime(1),
    );

    let waits = lock_waits_for(&logs.snapshot(), &connection_id);
    let [fields] = waits.as_slice() else {
        panic!("expected one slow lock wait, got {waits:?}");
    };
    assert_eq!(fields["operation"], json!("protocol_lock"));
    assert_eq!(fields["transport_operation_kind"], json!("request"));
    assert_eq!(fields["method"], json!(CLIENT_INITIALIZE));
    assert!(fields["wait_ms"].is_u64());
}

#[test]
fn uncontended_client_request_stays_quiet() {
    let logs = crate::logging::capture_test_logs();
    let connection_id = ConnectionId::new("conn-lock-wait-quiet");
    let gateway = SharedRpcGateway::new(gateway()).with_slow_lock_wait(NEVER_SLOW);

    gateway.handle_inbound(
        connection_id.clone(),
        request(
            "1",
            CLIENT_INITIALIZE,
            init_params("client-lock-wait-quiet"),
        ),
        AppServerTime(1),
    );

    assert!(lock_waits_for(&logs.snapshot(), &connection_id).is_empty());
}

fn lock_waits_for(
    logs: &[serde_json::Value],
    connection_id: &ConnectionId,
) -> Vec<serde_json::Value> {
    logs.iter()
        .filter(|line| line["event"] == json!("gateway_lock_wait_slow"))
        .filter(|line| line["fields"]["connection_id"] == json!(connection_id.as_str()))
        .map(|line| line["fields"].clone())
        .collect()
}
