//! `task/adoptNativeSession` and `task/reloadNativeSession` wait on the Agent for as long as
//! a `session/load` takes. They must not hold the protocol lock for it, or every other
//! client's requests and deliveries stall behind one slow load.

use std::sync::atomic::{AtomicBool, Ordering};

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::methods::{
    TASK_ADOPT_NATIVE_SESSION, TASK_RELOAD_NATIVE_SESSION,
};
use openaide_app_server_protocol::task::{
    TaskAdoptNativeSessionParams, TaskReloadNativeSessionParams,
};

use super::*;

/// Parks the workflow inside its Agent call until the test opens the gate.
#[derive(Default)]
struct Gate {
    entered: AtomicBool,
    open: AtomicBool,
}

impl Gate {
    fn pass(&self) {
        self.entered.store(true, Ordering::SeqCst);
        crate::test_sync::hold_while(|| !self.open.load(Ordering::SeqCst));
    }

    fn wait_entered(&self) {
        crate::test_sync::wait_until("the workflow reaches the Agent", || {
            self.entered.load(Ordering::SeqCst)
        });
    }

    fn open(&self) {
        self.open.store(true, Ordering::SeqCst);
    }
}

fn finished() -> openaide_app_server_protocol::errors::ProtocolError {
    openaide_app_server_protocol::errors::ProtocolError {
        code: ProtocolErrorCode::Conflict,
        message: "workflow finished".to_string(),
        recoverable: true,
        target: None,
    }
}

struct GatedAdoption(Arc<Gate>);

impl TaskAdoptNativeSessionWorkflow for GatedAdoption {
    fn adopt_native_session(
        &self,
        _params: TaskAdoptNativeSessionParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        self.0.pass();
        Err(finished())
    }
}

struct GatedReload(Arc<Gate>);

impl TaskOpenWorkflow for GatedReload {
    fn open_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskOpenParams,
    ) -> Result<(), ProtocolError> {
        Err(finished())
    }

    fn mark_read_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskMarkReadParams,
    ) -> Result<(), ProtocolError> {
        Err(finished())
    }

    fn reload_native_session_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: TaskReloadNativeSessionParams,
    ) -> Result<(), ProtocolError> {
        self.0.pass();
        Err(finished())
    }
}

fn adopt_request() -> InboundProtocolMessage {
    request(
        "adopt-1",
        TASK_ADOPT_NATIVE_SESSION,
        TaskAdoptNativeSessionParams {
            agent_id: "codex".into(),
            native_session_id: "native-1".to_string(),
        },
    )
}

fn reload_request() -> InboundProtocolMessage {
    request(
        "reload-1",
        TASK_RELOAD_NATIVE_SESSION,
        TaskReloadNativeSessionParams {
            task_id: "task-1".into(),
            client_mutation_id: "mutation-1".into(),
        },
    )
}

/// A shared gateway with two initialized clients, one per connection, whose adoption and
/// reload workflows park on `gate`.
fn gated_gateway(gate: &Arc<Gate>) -> SharedRpcGateway {
    let mut gateway = gateway();
    gateway.task_adopt_native_session = Arc::new(GatedAdoption(gate.clone()));
    gateway.task_open = Arc::new(GatedReload(gate.clone()));
    let gateway = SharedRpcGateway::new(gateway);
    for (connection, client) in [("conn-slow", "client-slow"), ("conn-other", "client-other")] {
        response_value(gateway.handle_inbound(
            ConnectionId::new(connection),
            request("init", CLIENT_INITIALIZE, init_params(client)),
            AppServerTime(1),
        ));
    }
    gateway
}

fn run_gated_request(
    build_request: fn() -> InboundProtocolMessage,
    detach_while_waiting: bool,
    expected: ProtocolErrorCode,
) {
    let gate = Arc::new(Gate::default());
    let gateway = gated_gateway(&gate);
    let slow = std::thread::spawn({
        let gateway = gateway.clone();
        move || {
            gateway.handle_inbound(
                ConnectionId::new("conn-slow"),
                build_request(),
                AppServerTime(2),
            )
        }
    });
    gate.wait_entered();

    // The workflow is blocked in the Agent: the protocol lock is free, and another client's
    // request is served now rather than after the load.
    assert!(
        gateway.try_lock_for_test().is_some(),
        "the protocol lock is held across the Agent call"
    );
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-other"),
        request("heartbeat", CLIENT_HEARTBEAT, json!({})),
        AppServerTime(3),
    ));
    if detach_while_waiting {
        response_value(gateway.handle_inbound(
            ConnectionId::new("conn-slow"),
            request("detach", CLIENT_DETACH, json!({})),
            AppServerTime(4),
        ));
    }

    gate.open();
    let error = response_error(slow.join().unwrap());
    assert_eq!(error.error.code, expected, "{error:?}");
}

#[test]
fn adoption_does_not_hold_the_protocol_lock_while_the_agent_loads() {
    run_gated_request(adopt_request, false, ProtocolErrorCode::Conflict);
}

#[test]
fn reload_does_not_hold_the_protocol_lock_while_the_agent_loads() {
    run_gated_request(reload_request, false, ProtocolErrorCode::Conflict);
}

#[test]
fn a_client_that_detaches_during_adoption_gets_no_reply_for_its_old_connection() {
    run_gated_request(adopt_request, true, ProtocolErrorCode::NotInitialized);
}
