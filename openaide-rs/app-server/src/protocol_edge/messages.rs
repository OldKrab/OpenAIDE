use openaide_app_server_protocol::envelopes::{ErrorEnvelope, RequestMeta};
use openaide_app_server_protocol::events::AppServerEvent;
use serde_json::Value;

use crate::client_lifecycle::{ConnectionId, Delivery};
use crate::server_requests::{ServerRequestAnswer, ServerRequestDelivery};

#[derive(Debug, Clone, PartialEq)]
pub enum InboundProtocolMessage {
    ClientRequest {
        id: String,
        method: String,
        params: Value,
        meta: RequestMeta,
    },
    ClientNotification {
        method: String,
        params: Value,
    },
    ClientResponse {
        request_id: String,
        answer: ServerRequestAnswer,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum GatewayOutcome {
    Respond {
        connection_id: ConnectionId,
        id: String,
        response: GatewayResponse,
        events: Vec<GatewayEventDelivery>,
        server_requests: Vec<ServerRequestDelivery>,
    },
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayEventDelivery {
    pub delivery: Delivery,
    pub event: AppServerEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayResponse {
    Result(Value),
    Error(Box<ErrorEnvelope>),
}

pub(crate) fn event_deliveries(
    outcome: crate::state_sync::PublishOutcome,
) -> Vec<GatewayEventDelivery> {
    outcome
        .deliveries
        .into_iter()
        .map(|published| GatewayEventDelivery {
            delivery: published.delivery,
            event: published.event,
        })
        .collect()
}
