use thiserror::Error;

use super::runner::{EndpointProbeError, EndpointProber};
use super::{EndpointProbeOutcome, EndpointProbeReport, EndpointRequirements, EndpointTarget};
use crate::storage_runtime::{RuntimeEndpoint, TransportKind};
use openaide_app_server_protocol::client::{ClientProbeLifecycle, ClientProbeResult};

pub mod exchange;

pub trait EndpointTransportProbe {
    fn supports_transport(&self, transport: TransportKind) -> bool;

    fn probe_endpoint(
        &mut self,
        endpoint: EndpointProbeEndpoint<'_>,
    ) -> Result<EndpointProbeObservation, EndpointTransportProbeError>;
}

#[derive(Debug, Clone, Copy)]
pub struct EndpointProbeEndpoint<'a> {
    pub endpoint: &'a RuntimeEndpoint,
    pub auth_token: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointProbeObservation {
    Alive(EndpointProbeFacts),
    Unreachable,
    AuthFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointProbeFacts {
    pub state_root_fingerprint: String,
    pub protocol_version: String,
    pub app_version: String,
    pub lifecycle: EndpointProbeLifecycle,
}

impl From<ClientProbeResult> for EndpointProbeFacts {
    fn from(result: ClientProbeResult) -> Self {
        Self {
            state_root_fingerprint: result.state_root_fingerprint,
            protocol_version: result.protocol_version,
            app_version: result.app_version,
            lifecycle: result.lifecycle.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointProbeLifecycle {
    Running,
    Draining,
    Stopping,
}

impl From<ClientProbeLifecycle> for EndpointProbeLifecycle {
    fn from(lifecycle: ClientProbeLifecycle) -> Self {
        match lifecycle {
            ClientProbeLifecycle::Running => Self::Running,
            ClientProbeLifecycle::Draining => Self::Draining,
            ClientProbeLifecycle::Stopping => Self::Stopping,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("endpoint transport probe failed: {message}")]
pub struct EndpointTransportProbeError {
    pub message: String,
}

pub struct EndpointProbeAdapter<T> {
    transport_probe: T,
}

impl<T> EndpointProbeAdapter<T> {
    pub fn new(transport_probe: T) -> Self {
        Self { transport_probe }
    }
}

impl<T: EndpointTransportProbe> EndpointProber for EndpointProbeAdapter<T> {
    fn probe(
        &mut self,
        target: EndpointTarget,
        requirements: EndpointRequirements,
    ) -> Result<EndpointProbeReport, EndpointProbeError> {
        for endpoint in &target.endpoints {
            if !self.transport_probe.supports_transport(endpoint.transport) {
                continue;
            }
            let observation = self
                .transport_probe
                .probe_endpoint(EndpointProbeEndpoint {
                    endpoint,
                    auth_token: &target.auth_token,
                })
                .map_err(|error| EndpointProbeError {
                    message: error.to_string(),
                })?;
            let outcome = classify_observation(&target, &requirements, observation);
            if outcome != EndpointProbeOutcome::Unreachable {
                return Ok(EndpointProbeReport {
                    target,
                    requirements,
                    outcome,
                });
            }
        }

        Ok(EndpointProbeReport {
            target,
            requirements,
            outcome: EndpointProbeOutcome::Unreachable,
        })
    }
}

pub fn classify_observation(
    target: &EndpointTarget,
    requirements: &EndpointRequirements,
    observation: EndpointProbeObservation,
) -> EndpointProbeOutcome {
    match observation {
        EndpointProbeObservation::Unreachable => EndpointProbeOutcome::Unreachable,
        EndpointProbeObservation::AuthFailed => EndpointProbeOutcome::AuthFailed,
        EndpointProbeObservation::Alive(facts) => {
            if facts.lifecycle == EndpointProbeLifecycle::Stopping {
                return EndpointProbeOutcome::ServerStopping;
            }
            if facts.state_root_fingerprint != target.state_root_fingerprint {
                return EndpointProbeOutcome::StateRootMismatch;
            }
            if facts.protocol_version != requirements.required_protocol_version {
                return EndpointProbeOutcome::IncompatibleProtocol;
            }
            if facts.app_version != requirements.required_app_version {
                return EndpointProbeOutcome::IncompatibleApp;
            }
            EndpointProbeOutcome::Compatible
        }
    }
}

#[cfg(test)]
mod tests;
