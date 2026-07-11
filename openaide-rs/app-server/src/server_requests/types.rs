use openaide_app_server_protocol::envelopes::ServerRequestEnvelope;
use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use serde_json::Value;

use crate::client_lifecycle::Delivery;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerRequestDraft {
    pub scope: PendingRequestScope,
    pub method: String,
    pub title: String,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerRequestDelivery {
    pub delivery: Delivery,
    pub envelope: ServerRequestEnvelope<Value>,
}

#[derive(Debug, Clone, PartialEq)]
// The outcome crosses the broker boundary infrequently and preserves a direct
// snapshot value; boxing would spread allocation concerns into every caller.
#[allow(clippy::large_enum_variant)]
pub enum OpenRequestOutcome {
    Opened {
        snapshot: openaide_app_server_protocol::snapshot::PendingRequestSnapshot,
        deliveries: Vec<ServerRequestDelivery>,
    },
    Unavailable {
        reason: RequestUnavailableReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestUnavailableReason {
    NoEligibleResponder,
    CapabilityUnavailable,
    UnsupportedMethod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerRequestAnswer {
    Result(Value),
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResponseOutcome {
    Accepted {
        request_id: RequestId,
        scope: PendingRequestScope,
        responder: ClientInstanceId,
        result: Value,
    },
    InvalidResponse {
        request_id: RequestId,
        responder: ClientInstanceId,
        message: String,
    },
    AlreadyResolved {
        request_id: RequestId,
    },
    UnknownRequest {
        request_id: RequestId,
    },
    UnauthorizedResponder {
        request_id: RequestId,
        responder: ClientInstanceId,
    },
    StaleRequest {
        request_id: RequestId,
        responder: ClientInstanceId,
    },
    Interrupted {
        request_id: RequestId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RequestLifecycleOutcome {
    Interrupted {
        request_id: RequestId,
        scope: PendingRequestScope,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponderScope {
    Client(ClientInstanceId),
    Task(openaide_app_server_protocol::ids::TaskId),
}
