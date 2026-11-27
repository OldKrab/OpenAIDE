//! Publish order is wire order (ADR-0086): a reply takes its place on the connection's outbound
//! sequence when App Server produces it, so no publication can overtake it or fall behind it on
//! the way to the wire, whichever thread writes.

use std::sync::mpsc;

use openaide_app_server_protocol::agent::{
    AgentAuthenticateParams, AgentAuthenticateResult, AgentAuthenticateStatus,
};
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::AgentCollectionSnapshot;

use super::*;
use crate::protocol_edge::outbox::OutboundMessage;

/// What reached the wire, in order: `reply:<id>` or `event`.
fn wire(gateway: &SharedRpcGateway, connection_id: &ConnectionId) -> Vec<String> {
    gateway.transfer_outbound(connection_id, |messages| {
        messages
            .into_iter()
            .map(|message| match message {
                OutboundMessage::Reply { id, .. } => format!("reply:{}", id.as_str().unwrap()),
                OutboundMessage::Event(_) => "event".to_string(),
                OutboundMessage::ServerRequest(_) => "server-request".to_string(),
            })
            .collect()
    })
}

fn subscribe_to_agents(gateway: &mut RpcGateway, connection_id: &ConnectionId) {
    response_value(gateway.handle_inbound(
        connection_id.clone(),
        request(
            "subscribe",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Agents,
                known_version: None,
            },
        ),
        AppServerTime(2),
    ));
}

fn subscribed_gateway() -> (SharedRpcGateway, ConnectionId) {
    let connection_id = ConnectionId::new("conn-1");
    let mut gateway = initialized_gateway("client-1", connection_id.as_str());
    subscribe_to_agents(&mut gateway, &connection_id);
    (SharedRpcGateway::new(gateway), connection_id)
}

fn heartbeat(id: &str) -> InboundProtocolMessage {
    request(id, CLIENT_HEARTBEAT, json!({}))
}

#[test]
fn a_publication_after_a_reply_is_produced_reaches_the_wire_after_it() {
    let (gateway, connection_id) = subscribed_gateway();

    gateway.dispatch_inbound(connection_id.clone(), heartbeat("3"), AppServerTime(3));
    // The reply is produced but not yet written when the background change is published.
    gateway.publish_agent_status_update(AppServerTime(4));

    assert_eq!(wire(&gateway, &connection_id), ["reply:3", "event"]);
}

#[test]
fn a_publication_before_a_reply_is_produced_reaches_the_wire_before_it() {
    let (gateway, connection_id) = subscribed_gateway();

    // Published but not yet written when the request arrives.
    gateway.publish_agent_status_update(AppServerTime(3));
    gateway.dispatch_inbound(connection_id.clone(), heartbeat("4"), AppServerTime(4));

    assert_eq!(wire(&gateway, &connection_id), ["event", "reply:4"]);
}

/// Authenticates only when the test releases it, so the request's slow, unlocked phase
/// stays open while the test publishes and transfers.
struct HeldAuthentication {
    started: Mutex<mpsc::Sender<()>>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl AgentAuthenticateWorkflow for HeldAuthentication {
    fn authenticate(
        &self,
        params: AgentAuthenticateParams,
    ) -> Result<AgentAuthenticateResult, ProtocolError> {
        self.started.lock().unwrap().send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(crate::test_sync::WATCHDOG)
            .expect("test released authentication");
        Ok(AgentAuthenticateResult {
            agent_id: params.agent_id,
            method_id: params.method_id,
            status: AgentAuthenticateStatus::AwaitingUser,
            agents: AgentCollectionSnapshot { agents: Vec::new() },
        })
    }
}

#[test]
fn a_request_running_outside_the_protocol_lock_keeps_its_reply_in_publish_order() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let connection_id = ConnectionId::new("conn-1");
    let mut raw = gateway_with_agent_authenticate(Arc::new(HeldAuthentication {
        started: Mutex::new(started_tx),
        release: Mutex::new(release_rx),
    }));
    initialize_client(&mut raw, connection_id.clone());
    subscribe_to_agents(&mut raw, &connection_id);
    let gateway = SharedRpcGateway::new(raw);
    drop(wire(&gateway, &connection_id));

    let authenticate = {
        let gateway = gateway.clone();
        let connection_id = connection_id.clone();
        std::thread::spawn(move || {
            gateway.dispatch_inbound(
                connection_id,
                request(
                    "auth",
                    AGENT_AUTHENTICATE,
                    json!({ "agentId": "codex", "methodId": "chat-gpt" }),
                ),
                AppServerTime(3),
            )
        })
    };
    started_rx
        .recv_timeout(crate::test_sync::WATCHDOG)
        .expect("authentication started");
    gateway.publish_agent_status_update(AppServerTime(4));
    // A link writing while the request is still running carries only what is already published.
    let during = wire(&gateway, &connection_id);
    release_tx.send(()).unwrap();
    assert!(authenticate.join().unwrap(), "authenticate queued a reply");
    gateway.publish_agent_status_update(AppServerTime(5));
    let after = wire(&gateway, &connection_id);

    assert_eq!(during, ["event"]);
    let reply = after
        .iter()
        .position(|message| message == "reply:auth")
        .expect("reply reached the wire");
    assert_eq!(after.last().map(String::as_str), Some("event"));
    assert!(reply < after.len() - 1, "{after:?}");
}
