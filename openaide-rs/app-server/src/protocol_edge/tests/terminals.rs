use openaide_app_server_protocol::client::{
    ClientReplacementImpactParams, UpdateShutdownPrepareParams,
};
use openaide_app_server_protocol::methods::{
    CLIENT_REPLACEMENT_IMPACT, TERMINALS_CLOSE, TERMINALS_OPEN,
};
use openaide_app_server_protocol::terminals::{
    TerminalTarget, TerminalsCloseParams, TerminalsOpenParams,
};
use serde_json::json;

use super::*;

#[cfg(unix)]
#[test]
fn a_terminal_opens_in_its_project_and_every_connected_client_sees_it_close() {
    let mut gateway = gateway_with_project_context();
    let folder = tempfile::tempdir().unwrap();
    let project = gateway
        .project_roots
        .add_project(folder.path().to_str().unwrap())
        .unwrap();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());
    assert!(gateway.terminals.snapshot().terminals.is_empty());
    gateway.drain_sync_for_connection(&connection_id);

    let outcome = gateway.handle_inbound(
        connection_id.clone(),
        request(
            "open",
            TERMINALS_OPEN,
            TerminalsOpenParams {
                target: TerminalTarget::Project {
                    project_id: project.project_id.clone(),
                },
                cols: 80,
                rows: 24,
            },
        ),
        AppServerTime(3),
    );
    let opened = response_value(outcome);
    let terminal_id = opened["result"]["terminal"]["terminalId"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        opened["result"]["terminal"]["projectId"],
        json!(project.project_id)
    );
    assert_eq!(opened["result"]["attach"]["path"], json!("/terminal"));
    let opened_sync = gateway.drain_sync_for_connection(&connection_id);
    let [changes] = opened_sync.as_slice() else {
        panic!("expected one sync notification, got {opened_sync:?}");
    };
    let terminals = changes.terminals.as_ref().expect("terminals changed");
    assert_eq!(terminals.terminals[0].terminal_id, terminal_id);

    response_value(gateway.handle_inbound(
        connection_id.clone(),
        request(
            "close",
            TERMINALS_CLOSE,
            TerminalsCloseParams {
                terminal_id,
                force: false,
            },
        ),
        AppServerTime(4),
    ));
    let closed_sync = gateway.drain_sync_for_connection(&connection_id);
    let [changes] = closed_sync.as_slice() else {
        panic!("expected one sync notification, got {closed_sync:?}");
    };
    assert!(matches!(
        &changes.terminals,
        Some(terminals) if terminals.terminals.is_empty()
    ));
}

#[test]
fn an_unknown_project_is_not_found() {
    let mut gateway = gateway_with_project_context();
    let connection_id = ConnectionId::new("conn-1");
    initialize_client(&mut gateway, connection_id.clone());

    let outcome = gateway.handle_inbound(
        connection_id,
        request(
            "open",
            TERMINALS_OPEN,
            TerminalsOpenParams {
                target: TerminalTarget::Project {
                    project_id: "missing".to_string().into(),
                },
                cols: 80,
                rows: 24,
            },
        ),
        AppServerTime(3),
    );
    assert!(gateway.terminals.snapshot().terminals.is_empty());
    assert_eq!(
        response_error(outcome).error.code,
        ProtocolErrorCode::NotFound
    );
}

#[cfg(unix)]
#[test]
fn an_open_terminal_is_reported_and_needs_confirmation_before_an_update() {
    let mut gateway = gateway();
    initialize_client(&mut gateway, ConnectionId::new("conn-1"));
    gateway
        .terminals
        .open(
            "project".to_string().into(),
            std::path::Path::new("/"),
            80,
            24,
            "project",
        )
        .unwrap();

    let impact = response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            CLIENT_REPLACEMENT_IMPACT,
            ClientReplacementImpactParams {},
        ),
        AppServerTime(2),
    ));
    assert_eq!(impact["result"]["openTerminals"], json!(1));

    let prepare = |gateway: &mut RpcGateway, id: &str, stop_active_work: bool| {
        response_value(gateway.handle_inbound(
            ConnectionId::new("conn-1"),
            request(
                id,
                CLIENT_UPDATE_SHUTDOWN_PREPARE,
                UpdateShutdownPrepareParams {
                    attempt_id: "update-1".to_string(),
                    stop_active_work,
                },
            ),
            AppServerTime(3),
        ))
    };
    assert_eq!(
        prepare(&mut gateway, "3", false)["result"],
        json!({ "kind": "blocked", "reason": "activeWork" })
    );
    assert_eq!(
        prepare(&mut gateway, "4", true)["result"],
        json!({ "kind": "ready" })
    );
    gateway.terminals.close_all();
}
