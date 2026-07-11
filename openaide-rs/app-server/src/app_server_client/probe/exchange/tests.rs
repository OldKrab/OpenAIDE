use serde_json::{json, Value};

use super::*;
use crate::storage_runtime::{RuntimeEndpoint, TransportKind};

#[test]
fn sends_typed_client_probe_request_and_maps_success_to_alive() {
    let mut transport = ClientProbeProtocolTransport::new(RecordingExchange::json(json!({
        "jsonrpc": "2.0",
        "id": "client_probe",
        "result": {
            "result": {
                "stateRootFingerprint": "root-a",
                "protocolVersion": "1",
                "appVersion": "0.1.0",
                "lifecycle": "running"
            },
            "meta": {}
        }
    })));

    let endpoint = runtime_endpoint();
    let observation = transport.probe_endpoint(probe_endpoint(&endpoint)).unwrap();

    assert_eq!(
        observation,
        EndpointProbeObservation::Alive(super::super::EndpointProbeFacts {
            state_root_fingerprint: "root-a".to_string(),
            protocol_version: "1".to_string(),
            app_version: "0.1.0".to_string(),
            lifecycle: super::super::EndpointProbeLifecycle::Running,
        })
    );
    assert_eq!(transport.exchange.requests.len(), 1);
    assert_eq!(
        transport.exchange.requests[0],
        json!({
            "jsonrpc": "2.0",
            "id": "client_probe",
            "method": "client/probe",
            "params": {},
            "meta": {}
        })
    );
    assert_eq!(transport.exchange.auth_tokens, vec!["token"]);
}

#[test]
fn preserves_transport_unreachable_and_auth_failed_observations() {
    let mut unreachable = ClientProbeProtocolTransport::new(RecordingExchange::response(
        ClientProbeExchangeResponse::Unreachable,
    ));
    let mut auth_failed = ClientProbeProtocolTransport::new(RecordingExchange::response(
        ClientProbeExchangeResponse::AuthFailed,
    ));
    let endpoint = runtime_endpoint();

    assert_eq!(
        unreachable
            .probe_endpoint(probe_endpoint(&endpoint))
            .unwrap(),
        EndpointProbeObservation::Unreachable
    );
    assert_eq!(
        auth_failed
            .probe_endpoint(probe_endpoint(&endpoint))
            .unwrap(),
        EndpointProbeObservation::AuthFailed
    );
}

#[test]
fn unauthorized_protocol_error_maps_to_auth_failed() {
    let mut transport = ClientProbeProtocolTransport::new(RecordingExchange::json(json!({
        "jsonrpc": "2.0",
        "id": "client_probe",
        "error": {
            "error": {
                "code": "unauthorized",
                "message": "bad token"
            },
            "meta": {}
        }
    })));
    let endpoint = runtime_endpoint();

    assert_eq!(
        transport.probe_endpoint(probe_endpoint(&endpoint)).unwrap(),
        EndpointProbeObservation::AuthFailed
    );
}

#[test]
fn malformed_or_mismatched_responses_fail_probe() {
    for response in [
        json!({"jsonrpc": "2.0", "id": "other", "result": {}}),
        json!({"jsonrpc": "1.0", "id": "client_probe", "result": {}}),
        json!({"jsonrpc": "2.0", "id": "client_probe"}),
        json!({"jsonrpc": "2.0", "id": "client_probe", "result": {"lifecycle": "running"}}),
        json!({
            "jsonrpc": "2.0",
            "id": "client_probe",
            "result": {"result": {"stateRootFingerprint": "root-a", "protocolVersion": "1", "appVersion": "0.1.0", "lifecycle": "running"}},
            "error": {"error": {"code": "unauthorized", "message": "bad token"}}
        }),
    ] {
        let mut transport = ClientProbeProtocolTransport::new(RecordingExchange::json(response));
        let endpoint = runtime_endpoint();
        assert!(transport.probe_endpoint(probe_endpoint(&endpoint)).is_err());
    }
}

#[test]
fn supports_transport_is_delegated_to_exchange() {
    let transport = ClientProbeProtocolTransport::new(
        RecordingExchange::json(json!({})).supporting([TransportKind::LocalHttp]),
    );

    assert!(transport.supports_transport(TransportKind::LocalHttp));
    assert!(!transport.supports_transport(TransportKind::Stdio));
}

#[test]
fn exchange_errors_fail_probe_without_fabricating_observation() {
    let mut transport = ClientProbeProtocolTransport::new(FailingExchange);
    let endpoint = runtime_endpoint();

    let error = transport
        .probe_endpoint(probe_endpoint(&endpoint))
        .unwrap_err();

    assert!(error.message.contains("socket closed"));
}

struct RecordingExchange {
    response: ClientProbeExchangeResponse,
    supported: Vec<TransportKind>,
    requests: Vec<Value>,
    auth_tokens: Vec<String>,
}

impl RecordingExchange {
    fn json(response: Value) -> Self {
        Self::response(ClientProbeExchangeResponse::Json(response))
    }

    fn response(response: ClientProbeExchangeResponse) -> Self {
        Self {
            response,
            supported: vec![TransportKind::LocalHttp, TransportKind::Stdio],
            requests: Vec::new(),
            auth_tokens: Vec::new(),
        }
    }

    fn supporting(mut self, transports: impl IntoIterator<Item = TransportKind>) -> Self {
        self.supported = transports.into_iter().collect();
        self
    }
}

impl ClientProbeExchange for RecordingExchange {
    fn supports_transport(&self, transport: TransportKind) -> bool {
        self.supported.contains(&transport)
    }

    fn exchange(
        &mut self,
        endpoint: ClientProbeExchangeEndpoint<'_>,
        request: Value,
    ) -> Result<ClientProbeExchangeResponse, ClientProbeExchangeError> {
        self.requests.push(request);
        self.auth_tokens
            .push(endpoint.endpoint.auth_token.to_string());
        Ok(self.response.clone())
    }
}

struct FailingExchange;

impl ClientProbeExchange for FailingExchange {
    fn supports_transport(&self, _transport: TransportKind) -> bool {
        true
    }

    fn exchange(
        &mut self,
        _endpoint: ClientProbeExchangeEndpoint<'_>,
        _request: Value,
    ) -> Result<ClientProbeExchangeResponse, ClientProbeExchangeError> {
        Err(ClientProbeExchangeError {
            message: "socket closed".to_string(),
        })
    }
}

fn runtime_endpoint() -> RuntimeEndpoint {
    RuntimeEndpoint {
        transport: TransportKind::LocalHttp,
        address: "http://127.0.0.1:1".to_string(),
    }
}

fn probe_endpoint(endpoint: &RuntimeEndpoint) -> EndpointProbeEndpoint<'_> {
    EndpointProbeEndpoint {
        endpoint,
        auth_token: "token",
    }
}
