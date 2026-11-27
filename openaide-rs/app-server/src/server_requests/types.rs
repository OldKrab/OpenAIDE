use openaide_app_server_protocol::envelopes::ServerRequestEnvelope;
use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use serde_json::Value;

use crate::client_lifecycle::Delivery;

/// Tags naming a Permission or Question request's kind inside App Server. They never appear on
/// the wire: such a request reaches clients only as Task state and is answered with
/// `pendingRequest/resolve` (ADR-0086).
pub const PERMISSION_REQUEST_TAG: &str = "permission/request";
pub const QUESTION_REQUEST_TAG: &str = "question/request";

/// Whether a request with this tag is delivered as Task state instead of a server request.
pub fn is_task_state_request(tag: &str) -> bool {
    matches!(tag, PERMISSION_REQUEST_TAG | QUESTION_REQUEST_TAG)
}

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
