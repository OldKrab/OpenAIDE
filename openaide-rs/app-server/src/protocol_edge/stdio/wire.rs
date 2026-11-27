use openaide_app_server_protocol::envelopes::{ErrorEnvelope, RequestMeta, ResponseMeta};
use openaide_app_server_protocol::errors::{ErrorTarget, ProtocolError, ProtocolErrorCode};
use serde::de::Error as _;
use serde::Deserialize;
use serde_json::Value;

use crate::protocol_edge::outbox::OutboundMessage;
use crate::protocol_edge::{GatewayResponse, InboundProtocolMessage};
use crate::server_requests::ServerRequestAnswer;

#[derive(Debug, Deserialize)]
pub(crate) struct WireRequest {
    pub jsonrpc: String,
    #[serde(default, deserialize_with = "deserialize_wire_request_id")]
    pub id: WireRequestId,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub meta: RequestMeta,
}

#[derive(Debug, Default)]
pub(crate) enum WireRequestId {
    #[default]
    Notification,
    Request(Value),
    Invalid,
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub(crate) enum WireMessage {
    Response(WireResponse),
    Notification(WireNotification),
    Request(WireServerRequest),
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct WireResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorEnvelope>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct WireNotification {
    jsonrpc: &'static str,
    method: &'static str,
    params: Value,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct WireServerRequest {
    jsonrpc: &'static str,
    id: String,
    scope: openaide_app_server_protocol::snapshot::PendingRequestScope,
    method: String,
    params: Value,
}

impl From<OutboundMessage> for WireMessage {
    fn from(message: OutboundMessage) -> Self {
        match message {
            OutboundMessage::Reply { id, response } => Self::Response(wire_response(id, response)),
            OutboundMessage::Event(event) => Self::Notification(WireNotification {
                jsonrpc: "2.0",
                method: "app/event",
                params: serde_json::to_value(event).expect("event serializes"),
            }),
            OutboundMessage::ServerRequest(envelope) => Self::Request(WireServerRequest {
                jsonrpc: "2.0",
                id: envelope.request_id.into_string(),
                scope: envelope.scope,
                method: envelope.method,
                params: envelope.params,
            }),
        }
    }
}

/// An inbound message refused before dispatch. `reason_code` is a stable, non-sensitive label
/// for diagnostics; the reply answers `id` with an invalid-request error.
#[derive(Debug)]
pub(crate) struct InboundRejection {
    pub reason_code: &'static str,
    id: Value,
    message: String,
}

impl InboundRejection {
    fn new(reason_code: &'static str, id: Option<Value>, message: String) -> Self {
        Self {
            reason_code,
            id: id.unwrap_or(Value::Null),
            message,
        }
    }

    pub(crate) fn invalid_request(
        reason_code: &'static str,
        id: Option<Value>,
        detail: impl std::fmt::Display,
    ) -> Self {
        Self::new(reason_code, id, format!("Invalid request: {detail}"))
    }

    pub(crate) fn into_wire(self) -> WireMessage {
        WireMessage::from(self.into_outbound())
    }

    pub(crate) fn into_outbound(self) -> OutboundMessage {
        OutboundMessage::Reply {
            id: self.id,
            response: GatewayResponse::Error(Box::new(invalid_request_envelope(self.message))),
        }
    }
}

pub(crate) fn parse_rejection(error: serde_json::Error) -> InboundRejection {
    InboundRejection::new("malformed_json", None, format!("Parse error: {error}"))
}

/// Reads one JSON-RPC message from a client: a response to a server request, a request, or a
/// notification.
pub(crate) fn parse_inbound(value: Value) -> Result<InboundProtocolMessage, InboundRejection> {
    if let Some(response) = client_response(&value) {
        return Ok(response);
    }
    let request = serde_json::from_value::<WireRequest>(value).map_err(|error| {
        InboundRejection::invalid_request("invalid_request_envelope", None, error)
    })?;
    let invalid = |reason_code, id: Option<&Value>, detail: &str| {
        InboundRejection::invalid_request(reason_code, id.cloned(), detail)
    };
    let id = match request.id {
        WireRequestId::Notification => None,
        WireRequestId::Invalid => {
            return Err(invalid(
                "invalid_request_id",
                Some(&Value::Null),
                "invalid JSON-RPC id",
            ))
        }
        WireRequestId::Request(id) => Some(id),
    };
    if request.jsonrpc != "2.0" {
        return Err(invalid(
            "invalid_jsonrpc_version",
            id.as_ref(),
            "jsonrpc must be 2.0",
        ));
    }
    let Some(method) = request.method else {
        return Err(invalid("missing_method", id.as_ref(), "method is required"));
    };
    let params = request.params.unwrap_or_else(|| serde_json::json!({}));
    Ok(match id {
        Some(wire_id) => InboundProtocolMessage::ClientRequest {
            id: id_to_gateway_id(&wire_id),
            wire_id,
            method,
            params,
            meta: request.meta,
        },
        None => InboundProtocolMessage::ClientNotification { method, params },
    })
}

fn id_to_gateway_id(id: &Value) -> String {
    match id {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

pub(crate) fn client_response(value: &Value) -> Option<InboundProtocolMessage> {
    let record = value.as_object()?;
    if record.get("jsonrpc") != Some(&Value::String("2.0".to_string())) {
        return None;
    }
    if value.get("method").is_some() {
        return None;
    }
    let request_id = match value.get("id")? {
        Value::String(value) if value.starts_with("server-request-") => value.clone(),
        _ => return None,
    };
    if value.get("result").is_some() == value.get("error").is_some() {
        return None;
    }
    if let Some(result) = value.get("result") {
        return Some(InboundProtocolMessage::ClientResponse {
            request_id,
            answer: ServerRequestAnswer::Result(result.clone()),
        });
    }
    if let Some(error) = value.get("error") {
        return Some(InboundProtocolMessage::ClientResponse {
            request_id,
            answer: ServerRequestAnswer::Invalid(json_rpc_error_message(error)),
        });
    }
    None
}

pub(crate) fn serialize_message(message: WireMessage) -> String {
    serde_json::to_string(&message).unwrap_or_else(|_| {
        r#"{"jsonrpc":"2.0","id":null,"error":{"error":{"code":"internal","message":"serialization failed","recoverable":false}}}"#.to_string()
    })
}

fn deserialize_wire_request_id<'de, D>(deserializer: D) -> Result<WireRequestId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::String(_) | Value::Number(_) => WireRequestId::Request(value),
        Value::Null | Value::Array(_) | Value::Object(_) => WireRequestId::Invalid,
        Value::Bool(_) => {
            return Err(D::Error::custom(
                "JSON-RPC id must be a string, number, or absent",
            ));
        }
    })
}

fn wire_response(id: Value, response: GatewayResponse) -> WireResponse {
    match response {
        GatewayResponse::Result(result) => WireResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        },
        GatewayResponse::Error(error) => WireResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(*error),
        },
    }
}

fn invalid_request_envelope(message: String) -> ErrorEnvelope {
    ErrorEnvelope::new(
        ProtocolError {
            code: ProtocolErrorCode::InvalidRequest,
            message,
            recoverable: false,
            target: Some(ErrorTarget {
                method: None,
                field: None,
                current_task: None,
            }),
        },
        ResponseMeta::default(),
    )
}

fn json_rpc_error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("server request response error")
        .to_string()
}
