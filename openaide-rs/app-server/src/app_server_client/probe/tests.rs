use super::*;
use crate::app_server_client::runner::EndpointProber;
use openaide_app_server_protocol::client::{ClientProbeLifecycle, ClientProbeResult};

#[test]
fn compatible_observation_requires_matching_state_root_protocol_and_app() {
    assert_eq!(
        classify_observation(
            &target(vec![endpoint(TransportKind::LocalHttp)]),
            &requirements(),
            EndpointProbeObservation::Alive(facts("root-a", "1", "0.1.0"))
        ),
        EndpointProbeOutcome::Compatible
    );
}

#[test]
fn client_probe_result_maps_to_endpoint_probe_facts() {
    assert_eq!(
        EndpointProbeFacts::from(ClientProbeResult {
            state_root_fingerprint: "root-a".to_string(),
            protocol_version: "1".to_string(),
            app_version: "0.1.0".to_string(),
            lifecycle: ClientProbeLifecycle::Draining,
        }),
        EndpointProbeFacts {
            state_root_fingerprint: "root-a".to_string(),
            protocol_version: "1".to_string(),
            app_version: "0.1.0".to_string(),
            lifecycle: EndpointProbeLifecycle::Draining,
        }
    );
}

#[test]
fn classifier_reports_state_root_mismatch() {
    assert_eq!(
        classify_observation(
            &target(vec![endpoint(TransportKind::LocalHttp)]),
            &requirements(),
            EndpointProbeObservation::Alive(facts("root-b", "1", "0.1.0"))
        ),
        EndpointProbeOutcome::StateRootMismatch
    );
}

#[test]
fn classifier_reports_protocol_mismatch() {
    assert_eq!(
        classify_observation(
            &target(vec![endpoint(TransportKind::LocalHttp)]),
            &requirements(),
            EndpointProbeObservation::Alive(facts("root-a", "2", "0.1.0"))
        ),
        EndpointProbeOutcome::IncompatibleProtocol
    );
}

#[test]
fn classifier_reports_app_mismatch() {
    assert_eq!(
        classify_observation(
            &target(vec![endpoint(TransportKind::LocalHttp)]),
            &requirements(),
            EndpointProbeObservation::Alive(facts("root-a", "1", "0.2.0"))
        ),
        EndpointProbeOutcome::IncompatibleApp
    );
}

#[test]
fn classifier_reports_auth_failure_unreachable_and_stopping() {
    let target = target(vec![endpoint(TransportKind::LocalHttp)]);
    let requirements = requirements();

    assert_eq!(
        classify_observation(&target, &requirements, EndpointProbeObservation::AuthFailed),
        EndpointProbeOutcome::AuthFailed
    );
    assert_eq!(
        classify_observation(
            &target,
            &requirements,
            EndpointProbeObservation::Unreachable
        ),
        EndpointProbeOutcome::Unreachable
    );
    assert_eq!(
        classify_observation(
            &target,
            &requirements,
            EndpointProbeObservation::Alive(EndpointProbeFacts {
                lifecycle: EndpointProbeLifecycle::Stopping,
                ..facts("root-a", "1", "0.1.0")
            })
        ),
        EndpointProbeOutcome::ServerStopping
    );
}

#[test]
fn protocol_lifecycle_mapping_classifies_reuse_and_shutdown_states() {
    let cases = [
        (
            ClientProbeLifecycle::Running,
            EndpointProbeOutcome::Compatible,
        ),
        (
            ClientProbeLifecycle::Draining,
            EndpointProbeOutcome::Compatible,
        ),
        (
            ClientProbeLifecycle::Stopping,
            EndpointProbeOutcome::ServerStopping,
        ),
    ];

    for (lifecycle, expected) in cases {
        let facts = EndpointProbeFacts::from(client_probe_result(lifecycle));
        assert_eq!(
            classify_observation(
                &target(vec![endpoint(TransportKind::LocalHttp)]),
                &requirements(),
                EndpointProbeObservation::Alive(facts),
            ),
            expected,
        );
    }
}

#[test]
fn adapter_skips_unsupported_transport_and_uses_supported_endpoint() {
    let mut adapter = EndpointProbeAdapter::new(
        RecordingProbe::new(vec![EndpointProbeObservation::Alive(facts(
            "root-a", "1", "0.1.0",
        ))])
        .supporting([TransportKind::LocalHttp]),
    );
    let target = target(vec![
        endpoint(TransportKind::Stdio),
        endpoint(TransportKind::LocalHttp),
    ]);

    let report = adapter.probe(target.clone(), requirements()).unwrap();

    assert_eq!(report.target, target);
    assert_eq!(report.requirements, requirements());
    assert_eq!(report.outcome, EndpointProbeOutcome::Compatible);
    assert_eq!(
        adapter.transport_probe.probed_transports,
        vec![TransportKind::LocalHttp]
    );
    assert_eq!(adapter.transport_probe.auth_tokens, vec!["token"]);
}

#[test]
fn adapter_reports_unreachable_when_all_supported_endpoints_are_unreachable() {
    let mut adapter = EndpointProbeAdapter::new(
        RecordingProbe::new(vec![EndpointProbeObservation::Unreachable])
            .supporting([TransportKind::LocalHttp]),
    );

    let report = adapter
        .probe(
            target(vec![endpoint(TransportKind::LocalHttp)]),
            requirements(),
        )
        .unwrap();

    assert_eq!(report.outcome, EndpointProbeOutcome::Unreachable);
}

#[test]
fn adapter_reports_unreachable_when_no_transport_is_supported() {
    let mut adapter = EndpointProbeAdapter::new(
        RecordingProbe::new(vec![EndpointProbeObservation::Alive(facts(
            "root-a", "1", "0.1.0",
        ))])
        .supporting([TransportKind::LocalHttp]),
    );

    let report = adapter
        .probe(target(vec![endpoint(TransportKind::Stdio)]), requirements())
        .unwrap();

    assert_eq!(report.outcome, EndpointProbeOutcome::Unreachable);
    assert!(adapter.transport_probe.probed_transports.is_empty());
}

#[test]
fn adapter_propagates_transport_probe_error_without_fabricating_report() {
    let mut adapter = EndpointProbeAdapter::new(FailingProbe);

    let error = adapter
        .probe(
            target(vec![endpoint(TransportKind::LocalHttp)]),
            requirements(),
        )
        .unwrap_err();

    assert!(error.message.contains("boom"));
}

struct RecordingProbe {
    observations: Vec<EndpointProbeObservation>,
    supported_transports: Vec<TransportKind>,
    probed_transports: Vec<TransportKind>,
    auth_tokens: Vec<String>,
}

impl RecordingProbe {
    fn new(observations: Vec<EndpointProbeObservation>) -> Self {
        Self {
            observations,
            supported_transports: Vec::new(),
            probed_transports: Vec::new(),
            auth_tokens: Vec::new(),
        }
    }

    fn supporting(mut self, transports: impl IntoIterator<Item = TransportKind>) -> Self {
        self.supported_transports = transports.into_iter().collect();
        self
    }
}

impl EndpointTransportProbe for RecordingProbe {
    fn supports_transport(&self, transport: TransportKind) -> bool {
        self.supported_transports.contains(&transport)
    }

    fn probe_endpoint(
        &mut self,
        endpoint: EndpointProbeEndpoint<'_>,
    ) -> Result<EndpointProbeObservation, EndpointTransportProbeError> {
        self.probed_transports.push(endpoint.endpoint.transport);
        self.auth_tokens.push(endpoint.auth_token.to_string());
        Ok(self.observations.remove(0))
    }
}

struct FailingProbe;

impl EndpointTransportProbe for FailingProbe {
    fn supports_transport(&self, transport: TransportKind) -> bool {
        transport == TransportKind::LocalHttp
    }

    fn probe_endpoint(
        &mut self,
        _endpoint: EndpointProbeEndpoint<'_>,
    ) -> Result<EndpointProbeObservation, EndpointTransportProbeError> {
        Err(EndpointTransportProbeError {
            message: "boom".to_string(),
        })
    }
}

fn target(endpoints: Vec<RuntimeEndpoint>) -> EndpointTarget {
    EndpointTarget {
        server_id: "server-1".to_string(),
        state_root_fingerprint: "root-a".to_string(),
        protocol_version: "1".to_string(),
        app_version: "0.1.0".to_string(),
        auth_token: "token".to_string(),
        replacement_token: Some("replacement-token".to_string()),
        endpoints,
    }
}

fn requirements() -> EndpointRequirements {
    EndpointRequirements {
        required_protocol_version: "1".to_string(),
        required_app_version: "0.1.0".to_string(),
    }
}

fn endpoint(transport: TransportKind) -> RuntimeEndpoint {
    RuntimeEndpoint {
        transport,
        address: "endpoint-address".to_string(),
    }
}

fn facts(root: &str, protocol: &str, app: &str) -> EndpointProbeFacts {
    EndpointProbeFacts {
        state_root_fingerprint: root.to_string(),
        protocol_version: protocol.to_string(),
        app_version: app.to_string(),
        lifecycle: EndpointProbeLifecycle::Running,
    }
}

fn client_probe_result(lifecycle: ClientProbeLifecycle) -> ClientProbeResult {
    ClientProbeResult {
        state_root_fingerprint: "root-a".to_string(),
        protocol_version: "1".to_string(),
        app_version: "0.1.0".to_string(),
        lifecycle,
    }
}
