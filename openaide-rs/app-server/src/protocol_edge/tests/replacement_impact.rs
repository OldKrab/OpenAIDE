use openaide_app_server_protocol::client::ClientReplacementImpactParams;
use openaide_app_server_protocol::methods::CLIENT_REPLACEMENT_IMPACT;
use openaide_app_server_protocol::snapshot::ProtocolVersion;
use serde_json::json;

use super::*;

#[test]
fn reports_other_clients_and_active_work_without_starting_a_replacement() {
    let mut gateway = gateway();
    gateway.shutdown = Arc::new(FixedShutdownWithBlockers {
        active_turns: 2,
        pending_task_requests: 1,
    });
    initialize_client(&mut gateway, ConnectionId::new("conn-1"));
    let mut phone = init_params("client-2");
    phone.shell = ShellDescriptor {
        kind: ShellKind::Mobile,
        name: Some("Pixel".to_string()),
        version: Some("0.9.0".to_string()),
    };
    // An older compatible minor: same major as this App Server, minor 0.
    phone.protocol_version = Some(ProtocolVersion {
        major: openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION.major,
        minor: 0,
    });
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request("2", CLIENT_INITIALIZE, phone),
        AppServerTime(2),
    ));
    let mut terminal = init_params("client-3");
    terminal.shell = ShellDescriptor {
        kind: ShellKind::Terminal,
        name: None,
        version: Some("0.10.0".to_string()),
    };
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-3"),
        request("3", CLIENT_INITIALIZE, terminal),
        AppServerTime(3),
    ));
    gateway.handle_transport_closed(&ConnectionId::new("conn-3"), AppServerTime(4));

    let impact = response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "4",
            CLIENT_REPLACEMENT_IMPACT,
            ClientReplacementImpactParams {},
        ),
        AppServerTime(5),
    ));

    assert_eq!(
        impact["result"],
        json!({
            "otherClients": [
                {
                    "kind": "mobile",
                    "name": "Pixel",
                    "appVersion": "0.9.0",
                    "protocolVersion": {
                        "major": openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION.major,
                        "minor": 0
                    },
                    "reconnecting": false
                },
                { "kind": "terminal", "appVersion": "0.10.0", "reconnecting": true }
            ],
            "activeTurns": 2,
            "pendingTaskRequests": 1,
            "openTerminals": 0
        })
    );
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request(
            "5",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Projects,
                known_revision: None,
            },
        ),
        AppServerTime(6),
    ));
}

#[test]
fn a_lone_idle_client_sees_no_impact() {
    let mut gateway = initialized_gateway("client-1", "conn-1");

    let impact = response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            CLIENT_REPLACEMENT_IMPACT,
            ClientReplacementImpactParams {},
        ),
        AppServerTime(2),
    ));

    assert_eq!(
        impact["result"],
        json!({ "activeTurns": 0, "pendingTaskRequests": 0, "openTerminals": 0 })
    );
}

#[test]
fn requires_an_initialized_client() {
    let mut gateway = gateway();

    let error = response_error(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "1",
            CLIENT_REPLACEMENT_IMPACT,
            ClientReplacementImpactParams {},
        ),
        AppServerTime(1),
    ));

    assert_eq!(error.error.code, ProtocolErrorCode::NotInitialized);
}
