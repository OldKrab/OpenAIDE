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
        json!(openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION.to_string())
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

#[test]
fn initialize_refuses_a_client_built_for_a_newer_protocol() {
    use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION as SERVER;
    use openaide_app_server_protocol::snapshot::ProtocolVersion;

    let mut gateway = gateway();
    let mut params = init_params("client-1");
    params.protocol_version = Some(ProtocolVersion {
        major: SERVER.major,
        minor: SERVER.minor + 1,
    });

    let error = response_error(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("1", CLIENT_INITIALIZE, params),
        AppServerTime(1),
    ));

    assert_eq!(error.error.code, ProtocolErrorCode::IncompatibleProtocol);
    assert_eq!(error.error.message, "Update OpenAIDE on your computer");
    assert!(gateway
        .client_hub
        .context_for_connection(&ConnectionId::new("conn-1"))
        .is_none());
}

#[test]
fn initialize_accepts_a_client_built_for_an_older_minor() {
    use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION as SERVER;
    use openaide_app_server_protocol::snapshot::ProtocolVersion;

    let mut gateway = gateway();
    let mut params = init_params("client-1");
    params.protocol_version = Some(ProtocolVersion {
        major: SERVER.major,
        minor: 0,
    });

    let value = response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("1", CLIENT_INITIALIZE, params),
        AppServerTime(1),
    ));

    assert_eq!(
        value["result"]["snapshot"]["server"]["protocolVersion"],
        json!({ "major": SERVER.major, "minor": SERVER.minor })
    );
}
