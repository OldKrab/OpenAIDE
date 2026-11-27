//! Each client connection's one outbound sequence (ADR-0086, "Publish order is wire order").
//!
//! Every reply, event, and server request for a connection is appended here at the moment App
//! Server produces it, while the protocol lock is still held, so the append order is the order of
//! App Server's decisions. A transport moves a connection's queue to its wire with
//! [`Outbox::transfer`], which holds that connection's transfer lock across the drain and the
//! write: two transfers for one connection cannot interleave, and nothing reaches a wire except
//! through here.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use openaide_app_server_protocol::envelopes::ServerRequestEnvelope;
use openaide_app_server_protocol::events::AppServerEvent;
use serde_json::{json, Value};

use crate::client_lifecycle::ConnectionId;
use crate::logging;
use crate::server_requests::ServerRequestDelivery;

use super::{GatewayEventDelivery, GatewayOutcome, GatewayResponse, InboundProtocolMessage};

/// A queue this long means a connection stopped transferring while App Server kept publishing.
const LARGE_BACKLOG: usize = 1_024;
/// A transfer this slow holds back every later message for its connection.
const SLOW_TRANSFER: Duration = Duration::from_millis(100);

/// One message on a connection's outbound sequence, already routed to that connection.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OutboundMessage {
    Reply {
        id: Value,
        response: GatewayResponse,
    },
    /// Boxed: an event is far larger than a reply header or a server request.
    Event(Box<AppServerEvent>),
    ServerRequest(ServerRequestEnvelope<Value>),
}

impl OutboundMessage {
    pub(crate) fn server_request_method(&self) -> Option<&str> {
        match self {
            Self::ServerRequest(envelope) => Some(envelope.method.as_str()),
            _ => None,
        }
    }
}

/// Whether, and under which wire id, an inbound message is answered.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplyRoute {
    /// A client request is always answered.
    Request(Value),
    /// A client's answer to a server request is answered only when App Server rejects it.
    Rejection(Value),
    /// A client notification is never answered.
    None,
}

impl ReplyRoute {
    pub(crate) fn for_message(message: &InboundProtocolMessage) -> Self {
        match message {
            InboundProtocolMessage::ClientRequest { wire_id, .. } => Self::Request(wire_id.clone()),
            InboundProtocolMessage::ClientResponse { request_id, .. } => {
                Self::Rejection(Value::String(request_id.clone()))
            }
            InboundProtocolMessage::ClientNotification { .. } => Self::None,
        }
    }

    fn reply_id(self, response: &GatewayResponse) -> Option<Value> {
        match self {
            Self::Request(id) => Some(id),
            Self::Rejection(id) if matches!(response, GatewayResponse::Error(_)) => Some(id),
            Self::Rejection(_) | Self::None => None,
        }
    }
}

#[derive(Default)]
struct ConnectionQueue {
    messages: VecDeque<OutboundMessage>,
    /// Held across a drain and its write. Shared by `Arc` so a writer can wait on it without
    /// holding the map, and so pushes never wait on a slow writer.
    transfer: Arc<Mutex<()>>,
}

#[derive(Clone, Default)]
pub(crate) struct Outbox {
    connections: Arc<Mutex<HashMap<ConnectionId, ConnectionQueue>>>,
}

impl Outbox {
    pub(crate) fn push(&self, connection_id: &ConnectionId, message: OutboundMessage) {
        let mut connections = self.connections.lock().expect("outbox poisoned");
        let queue = connections.entry(connection_id.clone()).or_default();
        queue.messages.push_back(message);
        if queue.messages.len() == LARGE_BACKLOG {
            logging::warn(
                "outbox_backlog_large",
                json!({
                    "connection_id": connection_id.as_str(),
                    "queued_count": queue.messages.len(),
                }),
            );
        }
    }

    pub(crate) fn push_events(&self, events: impl IntoIterator<Item = GatewayEventDelivery>) {
        for event in events {
            self.push(
                &event.delivery.connection_id,
                OutboundMessage::Event(Box::new(event.event)),
            );
        }
    }

    pub(crate) fn push_server_requests(
        &self,
        requests: impl IntoIterator<Item = ServerRequestDelivery>,
    ) {
        for request in requests {
            self.push(
                &request.delivery.connection_id,
                OutboundMessage::ServerRequest(request.envelope),
            );
        }
    }

    /// Appends what one inbound message produced. Its events and server requests go first, so
    /// a reply follows everything its request published; the reply goes to the requesting
    /// connection. Returns whether a reply was queued.
    pub(crate) fn push_outcome(&self, outcome: GatewayOutcome, route: ReplyRoute) -> bool {
        let GatewayOutcome::Respond {
            connection_id,
            response,
            events,
            server_requests,
            ..
        } = outcome
        else {
            return false;
        };
        self.push_events(events);
        self.push_server_requests(server_requests);
        let Some(id) = route.reply_id(&response) else {
            return false;
        };
        self.push(&connection_id, OutboundMessage::Reply { id, response });
        true
    }

    /// Moves the connection's queued messages to `write`, in order, holding the connection's
    /// transfer lock until `write` returns.
    pub(crate) fn transfer<R>(
        &self,
        connection_id: &ConnectionId,
        write: impl FnOnce(Vec<OutboundMessage>) -> R,
    ) -> R {
        let transfer = self
            .connections
            .lock()
            .expect("outbox poisoned")
            .entry(connection_id.clone())
            .or_default()
            .transfer
            .clone();
        let _held = transfer.lock().expect("outbox transfer poisoned");
        let started_at = Instant::now();
        let messages: Vec<_> = self
            .connections
            .lock()
            .expect("outbox poisoned")
            .get_mut(connection_id)
            .map(|queue| queue.messages.drain(..).collect())
            .unwrap_or_default();
        let message_count = messages.len();
        let written = write(messages);
        let elapsed = started_at.elapsed();
        if elapsed >= SLOW_TRANSFER {
            logging::info(
                "outbox_transfer_slow",
                json!({
                    "connection_id": connection_id.as_str(),
                    "message_count": message_count,
                    "duration_ms": elapsed.as_millis(),
                }),
            );
        }
        self.forget_if_idle(connection_id, &transfer);
        written
    }

    /// Drops a connection's undelivered messages once no client can read them.
    pub(crate) fn discard(&self, connection_id: &ConnectionId, reason: &'static str) {
        let Some(queue) = self
            .connections
            .lock()
            .expect("outbox poisoned")
            .remove(connection_id)
        else {
            return;
        };
        logging::info(
            "outbox_discarded",
            json!({
                "connection_id": connection_id.as_str(),
                "discarded_count": queue.messages.len(),
                "reason": reason,
            }),
        );
    }

    /// Removes an empty queue nobody else is transferring, so connections that never
    /// initialize, such as one-shot HTTP callers, leave nothing behind. A transfer already
    /// waiting holds another reference to the lock and keeps the entry, so two transfers for one
    /// connection always share one lock.
    fn forget_if_idle(&self, connection_id: &ConnectionId, transfer: &Arc<Mutex<()>>) {
        let mut connections = self.connections.lock().expect("outbox poisoned");
        let idle = connections.get(connection_id).is_some_and(|queue| {
            queue.messages.is_empty()
                && Arc::ptr_eq(&queue.transfer, transfer)
                && Arc::strong_count(transfer) == 2
        });
        if idle {
            connections.remove(connection_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn queued(&self, connection_id: &ConnectionId) -> Vec<OutboundMessage> {
        self.connections
            .lock()
            .expect("outbox poisoned")
            .get(connection_id)
            .map(|queue| queue.messages.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// What a transport needs from App Server: run one inbound message, which appends its results to
/// the outbox, then move a connection's outbound sequence to the wire. Transport tests substitute
/// a fake that owns its own [`Outbox`].
pub(crate) trait ProtocolExchange {
    /// Runs one message. Returns whether a reply was queued.
    fn dispatch(&self, connection_id: ConnectionId, message: InboundProtocolMessage) -> bool;

    /// Renews the connection's client liveness for a live link and queues the server requests
    /// it can now answer. False when no initialized client owns the connection.
    fn observe_link(&self, connection_id: &ConnectionId) -> bool;

    fn transfer<R>(
        &self,
        connection_id: &ConnectionId,
        write: impl FnOnce(Vec<OutboundMessage>) -> R,
    ) -> R;
}

#[cfg(test)]
#[path = "outbox_tests.rs"]
mod tests;
