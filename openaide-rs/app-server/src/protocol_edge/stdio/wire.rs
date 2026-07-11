use openaide_app_server_protocol::envelopes::{ErrorEnvelope, RequestMeta, ResponseMeta};
use openaide_app_server_protocol::errors::{ErrorTarget, ProtocolError, ProtocolErrorCode};
use serde::de::Error as _;
use serde::Deserialize;
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::protocol_edge::{GatewayEventDelivery, GatewayResponse, InboundProtocolMessage};
use crate::server_requests::{ServerRequestAnswer, ServerRequestDelivery};

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

#[derive(Debug)]
pub(crate) enum WireRequestId {
    Notification,
    Request(Value),
    Invalid,
}

impl Default for WireRequestId {
    fn default() -> Self {
        Self::Notification
    }
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

pub(crate) fn wire_messages(
    id: Value,
    connection_id: ConnectionId,
    response: GatewayResponse,
    events: Vec<GatewayEventDelivery>,
    server_requests: Vec<ServerRequestDelivery>,
) -> Vec<WireMessage> {
    let mut messages = vec![WireMessage::Response(wire_response(id, response))];
    messages.extend(event_wire_messages(connection_id.clone(), events));
    messages.extend(server_request_wire_messages(connection_id, server_requests));
    messages
}

pub(crate) fn event_wire_messages(
    connection_id: ConnectionId,
    events: Vec<GatewayEventDelivery>,
) -> Vec<WireMessage> {
    events
        .into_iter()
        .filter(|event| event.delivery.connection_id == connection_id)
        .map(|event| {
            WireMessage::Notification(WireNotification {
                jsonrpc: "2.0",
                method: "app/event",
                params: serde_json::to_value(event.event).expect("event serializes"),
            })
        })
        .collect()
}

pub(crate) fn server_request_wire_messages(
    connection_id: ConnectionId,
    requests: Vec<ServerRequestDelivery>,
) -> Vec<WireMessage> {
    requests
        .into_iter()
        .filter(|request| request.delivery.connection_id == connection_id)
        .map(|request| {
            WireMessage::Request(WireServerRequest {
                jsonrpc: "2.0",
                id: request.envelope.request_id.into_string(),
                scope: request.envelope.scope,
                method: request.envelope.method,
                params: request.envelope.params,
            })
        })
        .collect()
}

pub(crate) fn parse_error(error: serde_json::Error) -> WireMessage {
    WireMessage::Response(error_response(Value::Null, format!("Parse error: {error}")))
}

pub(crate) fn invalid_request(id: Option<Value>, message: String) -> WireMessage {
    WireMessage::Response(error_response(
        id.unwrap_or(Value::Null),
        format!("Invalid request: {message}"),
    ))
}

pub(crate) fn id_to_gateway_id(id: &Value) -> String {
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
            error: Some(error),
        },
    }
}

fn error_response(id: Value, message: String) -> WireResponse {
    WireResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(ErrorEnvelope::new(
            ProtocolError {
                code: ProtocolErrorCode::InvalidRequest,
                message,
                recoverable: false,
                target: Some(ErrorTarget {
                    method: None,
                    field: None,
                }),
            },
            ResponseMeta::default(),
        )),
    }
}

fn json_rpc_error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("server request response error")
        .to_string()
}
