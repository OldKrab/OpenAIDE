use openaide_app_server_protocol::client::{ClientProbeParams, ClientProbeResult};
use openaide_app_server_protocol::envelopes::{ErrorEnvelope, RequestMeta, ResponseEnvelope};
use openaide_app_server_protocol::errors::ProtocolErrorCode;
use openaide_app_server_protocol::methods::{ClientProbe, ProtocolMethod};
use serde_json::{json, Value};

use super::{
    EndpointProbeEndpoint, EndpointProbeObservation, EndpointTransportProbe,
    EndpointTransportProbeError,
};
use crate::storage_runtime::TransportKind;

pub mod local_http;

const CLIENT_PROBE_REQUEST_ID: &str = "client_probe";

pub trait ClientProbeExchange {
    fn supports_transport(&self, transport: TransportKind) -> bool;

    fn exchange(
        &mut self,
        endpoint: ClientProbeExchangeEndpoint<'_>,
        request: Value,
    ) -> Result<ClientProbeExchangeResponse, ClientProbeExchangeError>;
}

#[derive(Debug, Clone, Copy)]
pub struct ClientProbeExchangeEndpoint<'a> {
    pub endpoint: EndpointProbeEndpoint<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientProbeExchangeResponse {
    Json(Value),
    Unreachable,
    AuthFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientProbeExchangeError {
    pub message: String,
}

pub struct ClientProbeProtocolTransport<T> {
    exchange: T,
}

impl<T> ClientProbeProtocolTransport<T> {
    pub fn new(exchange: T) -> Self {
        Self { exchange }
    }
}

impl<T: ClientProbeExchange> EndpointTransportProbe for ClientProbeProtocolTransport<T> {
    fn supports_transport(&self, transport: TransportKind) -> bool {
        self.exchange.supports_transport(transport)
    }

    fn probe_endpoint(
        &mut self,
        endpoint: EndpointProbeEndpoint<'_>,
    ) -> Result<EndpointProbeObservation, EndpointTransportProbeError> {
        let request = client_probe_request()?;
        let response = self
            .exchange
            .exchange(ClientProbeExchangeEndpoint { endpoint }, request)
            .map_err(transport_error)?;
        match response {
            ClientProbeExchangeResponse::Json(value) => parse_client_probe_response(value),
            ClientProbeExchangeResponse::Unreachable => Ok(EndpointProbeObservation::Unreachable),
            ClientProbeExchangeResponse::AuthFailed => Ok(EndpointProbeObservation::AuthFailed),
        }
    }
}

fn client_probe_request() -> Result<Value, EndpointTransportProbeError> {
    let envelope = ClientProbe::request(ClientProbeParams {}, RequestMeta::default());
    Ok(json!({
        "jsonrpc": "2.0",
        "id": CLIENT_PROBE_REQUEST_ID,
        "method": envelope.method,
        "params": envelope.params,
        "meta": envelope.meta,
    }))
}

fn parse_client_probe_response(
    value: Value,
) -> Result<EndpointProbeObservation, EndpointTransportProbeError> {
    validate_response_id(&value)?;
    let has_result = value.get("result").is_some();
    let has_error = value.get("error").is_some();
    if has_result && has_error {
        return Err(protocol_error(
            "client/probe response cannot contain both result and error",
        ));
    }

    if let Some(error) = value.get("error") {
        let envelope = serde_json::from_value::<ErrorEnvelope>(error.clone()).map_err(|error| {
            protocol_error(format!("invalid client/probe error envelope: {error}"))
        })?;
        return match envelope.error.code {
            ProtocolErrorCode::Unauthorized => Ok(EndpointProbeObservation::AuthFailed),
            _ => Err(protocol_error(format!(
                "client/probe returned protocol error: {:?}",
                envelope.error.code
            ))),
        };
    }

    let Some(result) = value.get("result") else {
        return Err(protocol_error(
            "client/probe response missing result or error",
        ));
    };
    let envelope = serde_json::from_value::<ResponseEnvelope<ClientProbeResult>>(result.clone())
        .map_err(|error| {
            protocol_error(format!("invalid client/probe result envelope: {error}"))
        })?;
    Ok(EndpointProbeObservation::Alive(envelope.result.into()))
}

fn validate_response_id(value: &Value) -> Result<(), EndpointTransportProbeError> {
    if value.get("jsonrpc") != Some(&Value::String("2.0".to_string())) {
        return Err(protocol_error("client/probe response jsonrpc must be 2.0"));
    }
    if value.get("id") != Some(&Value::String(CLIENT_PROBE_REQUEST_ID.to_string())) {
        return Err(protocol_error("client/probe response id mismatch"));
    }
    Ok(())
}

fn transport_error(error: ClientProbeExchangeError) -> EndpointTransportProbeError {
    EndpointTransportProbeError {
        message: error.message,
    }
}

fn protocol_error(message: impl Into<String>) -> EndpointTransportProbeError {
    EndpointTransportProbeError {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests;
