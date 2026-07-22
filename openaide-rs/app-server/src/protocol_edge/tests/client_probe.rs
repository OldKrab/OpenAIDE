use openaide_app_server_protocol::client::ClientProbeParams;
use openaide_app_server_protocol::methods::CLIENT_PROBE;
use serde_json::json;

use super::*;

#[test]
fn allowed_before_initialize_without_registering_client() {
    let mut gateway = gateway();

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("probe", CLIENT_PROBE, ClientProbeParams {}),
        AppServerTime(1),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["stateRootFingerprint"], json!("root-1"));
    assert_eq!(
        value["result"]["protocolVersion"],
        json!(openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION)
    );
    assert_eq!(
        value["result"]["appVersion"],
        json!(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(value["result"]["lifecycle"], json!("running"));
    assert!(gateway
        .client_hub
        .context_for_connection(&ConnectionId::new("conn-1"))
        .is_none());
}

#[test]
fn reports_stopping_without_initialize_admission_side_effects() {
    let mut gateway = gateway();
    gateway.lifecycle.begin_stopping();

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("probe", CLIENT_PROBE, ClientProbeParams {}),
        AppServerTime(1),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["lifecycle"], json!("stopping"));
}

#[test]
fn reports_draining_without_aborting_draining() {
    let mut gateway = gateway();
    gateway.lifecycle.begin_draining();

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("probe", CLIENT_PROBE, ClientProbeParams {}),
        AppServerTime(1),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["lifecycle"], json!("draining"));
}
