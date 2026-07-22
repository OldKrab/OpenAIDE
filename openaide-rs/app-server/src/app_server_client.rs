use std::fmt;

use crate::storage_runtime::{RuntimeEndpoint, RuntimeEndpointRecord, RuntimeEndpointRecordStatus};

pub mod launch_handoff;
pub mod probe;
pub(crate) mod replacement;
pub mod runner;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalHttpConnectionInfo {
    pub endpoint_url: String,
    pub auth_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachOrLaunchRequest {
    pub state_root_fingerprint: String,
    pub required_protocol_version: String,
    pub required_app_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointProbeOutcome {
    Compatible,
    IncompatibleProtocol,
    IncompatibleApp,
    AuthFailed,
    Unreachable,
    ServerStopping,
    StateRootMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointRequirements {
    pub required_protocol_version: String,
    pub required_app_version: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct EndpointTarget {
    pub server_id: String,
    pub state_root_fingerprint: String,
    pub protocol_version: String,
    pub app_version: String,
    pub auth_token: String,
    pub replacement_token: Option<String>,
    pub endpoints: Vec<RuntimeEndpoint>,
}

impl fmt::Debug for EndpointTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EndpointTarget")
            .field("server_id", &self.server_id)
            .field("state_root_fingerprint", &self.state_root_fingerprint)
            .field("protocol_version", &self.protocol_version)
            .field("app_version", &self.app_version)
            .field("auth_token", &"<redacted>")
            .field("replacement_token", &"<redacted>")
            .field("endpoints", &self.endpoints)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointProbeReport {
    pub target: EndpointTarget,
    pub requirements: EndpointRequirements,
    pub outcome: EndpointProbeOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchLockState {
    Acquired,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageWriterState {
    Available,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachOrLaunchDecision {
    ProbeEndpoint {
        target: EndpointTarget,
        requirements: EndpointRequirements,
    },
    AttachExisting {
        target: EndpointTarget,
    },
    LaunchNew,
    WaitForLaunch,
    CleanStaleEndpoint {
        target: EndpointTarget,
        reason: StaleEndpointReason,
    },
    ReplaceIncompatible {
        target: EndpointTarget,
        reason: AttachOrLaunchFailure,
    },
    Fail {
        reason: AttachOrLaunchFailure,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleEndpointReason {
    StateRootMismatch,
    EndpointRecordStopping,
    Unreachable,
    ProbeReportedStopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachOrLaunchFailure {
    IncompatibleProtocol,
    IncompatibleApp,
    AuthOrPermissionFailure,
    StorageStateRootBlocked,
}

pub struct AttachOrLaunchDecider;

impl AttachOrLaunchDecider {
    pub fn decide(
        request: &AttachOrLaunchRequest,
        endpoint: Option<&RuntimeEndpointRecord>,
        probe: Option<&EndpointProbeReport>,
        launch_lock: LaunchLockState,
        storage_writer: StorageWriterState,
    ) -> AttachOrLaunchDecision {
        if storage_writer == StorageWriterState::Blocked {
            return AttachOrLaunchDecision::Fail {
                reason: AttachOrLaunchFailure::StorageStateRootBlocked,
            };
        }

        if let Some(endpoint) = endpoint {
            let target = target(endpoint);
            if endpoint.state_root_fingerprint != request.state_root_fingerprint {
                return AttachOrLaunchDecision::CleanStaleEndpoint {
                    target,
                    reason: StaleEndpointReason::StateRootMismatch,
                };
            }
            if endpoint.status == RuntimeEndpointRecordStatus::Stopping {
                return AttachOrLaunchDecision::CleanStaleEndpoint {
                    target,
                    reason: StaleEndpointReason::EndpointRecordStopping,
                };
            }

            let requirements = requirements(request);
            let Some(probe) =
                probe.filter(|probe| probe.target == target && probe.requirements == requirements)
            else {
                return AttachOrLaunchDecision::ProbeEndpoint {
                    target,
                    requirements,
                };
            };
            return match probe.outcome {
                EndpointProbeOutcome::Compatible => {
                    AttachOrLaunchDecision::AttachExisting { target }
                }
                EndpointProbeOutcome::IncompatibleProtocol => {
                    AttachOrLaunchDecision::ReplaceIncompatible {
                        target,
                        reason: AttachOrLaunchFailure::IncompatibleProtocol,
                    }
                }
                EndpointProbeOutcome::IncompatibleApp => {
                    AttachOrLaunchDecision::ReplaceIncompatible {
                        target,
                        reason: AttachOrLaunchFailure::IncompatibleApp,
                    }
                }
                EndpointProbeOutcome::AuthFailed => AttachOrLaunchDecision::Fail {
                    reason: AttachOrLaunchFailure::AuthOrPermissionFailure,
                },
                EndpointProbeOutcome::Unreachable => AttachOrLaunchDecision::CleanStaleEndpoint {
                    target,
                    reason: StaleEndpointReason::Unreachable,
                },
                EndpointProbeOutcome::ServerStopping => {
                    AttachOrLaunchDecision::CleanStaleEndpoint {
                        target,
                        reason: StaleEndpointReason::ProbeReportedStopping,
                    }
                }
                EndpointProbeOutcome::StateRootMismatch => {
                    AttachOrLaunchDecision::CleanStaleEndpoint {
                        target,
                        reason: StaleEndpointReason::StateRootMismatch,
                    }
                }
            };
        }

        match launch_lock {
            LaunchLockState::Acquired => AttachOrLaunchDecision::LaunchNew,
            LaunchLockState::Busy => AttachOrLaunchDecision::WaitForLaunch,
        }
    }
}

fn requirements(request: &AttachOrLaunchRequest) -> EndpointRequirements {
    EndpointRequirements {
        required_protocol_version: request.required_protocol_version.clone(),
        required_app_version: request.required_app_version.clone(),
    }
}

fn target(endpoint: &RuntimeEndpointRecord) -> EndpointTarget {
    EndpointTarget {
        server_id: endpoint.server_id.clone(),
        state_root_fingerprint: endpoint.state_root_fingerprint.clone(),
        protocol_version: endpoint.protocol_version.clone(),
        app_version: endpoint.app_version.clone(),
        auth_token: endpoint.auth_token.clone(),
        replacement_token: endpoint.replacement_token.clone(),
        endpoints: endpoint.endpoints.clone(),
    }
}

#[cfg(test)]
mod tests;
