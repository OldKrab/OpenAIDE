use openaide_app_server_protocol::client::{
    ClientCapabilities, InitializeParams, RequestedSurface, ShellDescriptor, ShellKind,
};

use super::*;

#[test]
fn transport_close_enters_grace_before_expiry() {
    let mut hub = ClientHub::new(10);
    let client_id = ClientInstanceId::from("client-1");
    hub.initialize(
        ConnectionId::new("conn-1"),
        init_params(client_id.clone()),
        AppServerTime(100),
    );

    let closed = hub.observe_transport_closed(&ConnectionId::new("conn-1"), AppServerTime(101));
    assert_eq!(
        closed,
        TransportClosedOutcome::EnteredReconnectGrace {
            client_instance_id: client_id.clone(),
            expires_at: AppServerTime(111)
        }
    );
    assert_eq!(
        hub.expire_after_grace(&client_id, AppServerTime(110)),
        ClientExpiryOutcome::StillInGrace {
            expires_at: AppServerTime(111)
        }
    );
    assert_eq!(
        hub.expire_after_grace(&client_id, AppServerTime(111)),
        ClientExpiryOutcome::Expired {
            client_instance_id: client_id,
            last_client: true
        }
    );
}

#[test]
fn initialize_with_same_client_id_reattaches_delivery() {
    let mut hub = ClientHub::new(10);
    let client_id = ClientInstanceId::from("client-1");
    assert!(matches!(
        hub.initialize(
            ConnectionId::new("conn-1"),
            init_params(client_id.clone()),
            AppServerTime(1)
        ),
        InitializeClientOutcome::NewClient { .. }
    ));
    hub.observe_transport_closed(&ConnectionId::new("conn-1"), AppServerTime(2));

    let outcome = hub.initialize(
        ConnectionId::new("conn-2"),
        init_params(client_id.clone()),
        AppServerTime(3),
    );

    assert!(matches!(
        outcome,
        InitializeClientOutcome::ReattachedClient { .. }
    ));
    assert_eq!(
        hub.delivery_for(&client_id)
            .map(|delivery| delivery.connection_id),
        Some(ConnectionId::new("conn-2"))
    );
}

#[test]
fn reinitialize_replaces_old_live_connection_mapping() {
    let mut hub = ClientHub::new(10);
    let client_id = ClientInstanceId::from("client-1");
    hub.initialize(
        ConnectionId::new("conn-1"),
        init_params(client_id.clone()),
        AppServerTime(1),
    );
    hub.initialize(
        ConnectionId::new("conn-2"),
        init_params(client_id.clone()),
        AppServerTime(2),
    );

    assert_eq!(
        hub.observe_transport_closed(&ConnectionId::new("conn-1"), AppServerTime(3)),
        TransportClosedOutcome::UnknownConnection
    );
    assert_eq!(
        hub.expire_after_grace(&client_id, AppServerTime(20)),
        ClientExpiryOutcome::ClientConnected
    );
    assert_eq!(
        hub.delivery_for(&client_id)
            .map(|delivery| delivery.connection_id),
        Some(ConnectionId::new("conn-2"))
    );
}

#[test]
fn connection_activity_extends_liveness_deadline() {
    let mut hub = ClientHub::new(10);
    let client_id = ClientInstanceId::from("client-1");
    hub.initialize(
        ConnectionId::new("conn-1"),
        init_params(client_id.clone()),
        AppServerTime(1),
    );

    assert_eq!(
        hub.observe_connection_activity(&ConnectionId::new("conn-1"), AppServerTime(9)),
        Some(client_id.clone())
    );

    assert_eq!(
        hub.expire_inactive_clients(AppServerTime(10)),
        ClientExpiryBatch {
            expired: Vec::new(),
            last_client_expired: false,
        }
    );
    assert_eq!(
        hub.expire_inactive_clients(AppServerTime(19)),
        ClientExpiryBatch {
            expired: vec![client_id],
            last_client_expired: true,
        }
    );
}

#[test]
fn inactive_expiry_removes_only_due_clients() {
    let mut hub = ClientHub::new(10);
    let first = ClientInstanceId::from("client-1");
    let second = ClientInstanceId::from("client-2");
    hub.initialize(
        ConnectionId::new("conn-1"),
        init_params(first.clone()),
        AppServerTime(1),
    );
    hub.initialize(
        ConnectionId::new("conn-2"),
        init_params(second.clone()),
        AppServerTime(20),
    );

    assert_eq!(
        hub.expire_inactive_clients(AppServerTime(11)),
        ClientExpiryBatch {
            expired: vec![first.clone()],
            last_client_expired: false,
        }
    );
    assert!(hub.client_by_instance(&first).is_none());
    assert!(hub.client_by_instance(&second).is_some());
}

fn init_params(client_instance_id: ClientInstanceId) -> InitializeParams {
    InitializeParams {
        client_instance_id,
        shell: ShellDescriptor {
            kind: ShellKind::Web,
            name: None,
            version: None,
        },
        requested_surface: RequestedSurface::Home,
        capabilities: ClientCapabilities::default(),
    }
}
