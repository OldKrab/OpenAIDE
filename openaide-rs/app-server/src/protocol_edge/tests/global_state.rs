//! The global state through the gateway: who is sent what, and in how many notifications.

use super::*;

fn with_roots(client_id: &str, shell: ShellKind, roots: &[&str]) -> InitializeParams {
    let mut params = init_params(client_id);
    params.shell.kind = shell;
    params.workspace_roots = roots
        .iter()
        .map(|path| ClientWorkspaceRoot {
            path: (*path).to_string(),
        })
        .collect();
    params
}

fn initialize_with(
    gateway: &mut RpcGateway,
    connection: &ConnectionId,
    params: InitializeParams,
) -> Value {
    response_value(gateway.handle_inbound(
        connection.clone(),
        request("init", CLIENT_INITIALIZE, params),
        AppServerTime(1),
    ))
}

fn labels(value: &Value) -> Vec<String> {
    value["result"]["snapshot"]["global"]["projects"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|project| project["label"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn vscode_receives_only_the_projects_of_its_workspace_while_others_receive_all() {
    let mut gateway = gateway_with_project_context();
    let (vscode, web) = (
        ConnectionId::new("conn-vscode"),
        ConnectionId::new("conn-web"),
    );
    let first = initialize_with(
        &mut gateway,
        &vscode,
        with_roots(
            "client-vscode",
            ShellKind::VscodeExtension,
            &["/workspace/alpha"],
        ),
    );
    assert_eq!(labels(&first), ["alpha"]);
    gateway.drain_sync_for_connection(&vscode);

    let second = initialize_with(
        &mut gateway,
        &web,
        with_roots("client-web", ShellKind::Web, &["/workspace/beta"]),
    );

    assert_eq!(labels(&second).len(), 2, "a web client sees every project");
    assert!(
        gateway.drain_sync_for_connection(&vscode).is_empty(),
        "a project outside VS Code's workspace is not sent to it"
    );
}

#[test]
fn a_reconnecting_client_receives_the_full_state_not_a_delta() {
    let mut gateway = gateway_with_project_context();
    let (host, web) = (
        ConnectionId::new("conn-host"),
        ConnectionId::new("conn-web"),
    );
    initialize_with(
        &mut gateway,
        &host,
        with_roots("client-host", ShellKind::Web, &["/workspace/alpha"]),
    );
    initialize_with(&mut gateway, &web, init_params("client-web"));
    gateway.handle_transport_closed(&web, AppServerTime(2));

    let again = initialize_with(
        &mut gateway,
        &ConnectionId::new("conn-web-2"),
        init_params("client-web"),
    );

    assert_eq!(labels(&again), ["alpha"]);
}

#[test]
fn an_unchanged_global_state_publishes_nothing_to_connected_clients() {
    let mut gateway = gateway_with_project_context();
    let connection = ConnectionId::new("conn-1");
    initialize_with(&mut gateway, &connection, init_params("client-1"));
    gateway.drain_sync_for_connection(&connection);

    gateway.publish_global_state(crate::global_state::GlobalAreas::PROJECTS);
    gateway.publish_global_state(crate::global_state::GlobalAreas::SETTINGS);
    gateway.publish_global_state(crate::global_state::GlobalAreas::AGENTS);

    assert!(gateway.drain_sync_for_connection(&connection).is_empty());
}
