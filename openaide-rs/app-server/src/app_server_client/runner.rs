use std::path::PathBuf;

use thiserror::Error;

use super::{
    AttachOrLaunchDecider, AttachOrLaunchDecision, AttachOrLaunchFailure, AttachOrLaunchRequest,
    EndpointProbeReport, EndpointRequirements, EndpointTarget, LaunchLockState, StorageWriterState,
};
use crate::app_server_client::probe::exchange::local_http::LocalHttpProbeExchange;
use crate::app_server_client::probe::exchange::ClientProbeProtocolTransport;
use crate::app_server_client::probe::EndpointProbeAdapter;
use crate::storage_runtime::{
    EndpointRecordStore, EndpointRecordStoreError, LockAcquireOutcome, RuntimeLock,
    RuntimeLockError, StateRootFingerprint,
};

pub trait EndpointProber {
    fn probe(
        &mut self,
        target: EndpointTarget,
        requirements: EndpointRequirements,
    ) -> Result<EndpointProbeReport, EndpointProbeError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("endpoint probe failed: {message}")]
pub struct EndpointProbeError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachOrLaunchRequirements {
    pub required_protocol_version: String,
    pub required_app_version: String,
}

#[derive(Debug, Clone)]
pub struct AttachOrLaunchRunner {
    endpoint_records: EndpointRecordStore,
    launch_lock_path: PathBuf,
}

impl AttachOrLaunchRunner {
    pub fn new(
        endpoint_records: EndpointRecordStore,
        launch_lock_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            endpoint_records,
            launch_lock_path: launch_lock_path.into(),
        }
    }

    pub fn run<P: EndpointProber>(
        &self,
        fingerprint: &StateRootFingerprint,
        requirements: &AttachOrLaunchRequirements,
        storage_writer: StorageWriterState,
        prober: &mut P,
    ) -> Result<AttachOrLaunchRunResult, AttachOrLaunchRunError> {
        let request = AttachOrLaunchRequest {
            state_root_fingerprint: fingerprint.as_str().to_string(),
            required_protocol_version: requirements.required_protocol_version.clone(),
            required_app_version: requirements.required_app_version.clone(),
        };

        if storage_writer == StorageWriterState::Blocked {
            return Ok(AttachOrLaunchRunResult::Fail {
                reason: AttachOrLaunchFailure::StorageStateRootBlocked,
            });
        }

        let mut cleaned_once = false;
        loop {
            let endpoint = self.endpoint_records.read(fingerprint)?;
            if endpoint.is_none() {
                let lock = RuntimeLock::acquire(&self.launch_lock_path)?;
                return self.decide_without_endpoint(&request, storage_writer, lock);
            }

            let decision = AttachOrLaunchDecider::decide(
                &request,
                endpoint.as_ref(),
                None,
                LaunchLockState::Acquired,
                storage_writer,
            );
            match self.handle_endpoint_decision(&request, fingerprint, prober, decision)? {
                EndpointDecisionResult::Done(result) => return Ok(result),
                EndpointDecisionResult::RetryAfterCleanup => {
                    if cleaned_once {
                        return Err(AttachOrLaunchRunError::RepeatedStaleCleanup);
                    }
                    cleaned_once = true;
                }
                EndpointDecisionResult::RetryWithoutCleanup => {}
            }
        }
    }

    pub fn run_with_local_transports(
        &self,
        fingerprint: &StateRootFingerprint,
        requirements: &AttachOrLaunchRequirements,
        storage_writer: StorageWriterState,
    ) -> Result<AttachOrLaunchRunResult, AttachOrLaunchRunError> {
        let mut prober = EndpointProbeAdapter::new(ClientProbeProtocolTransport::new(
            LocalHttpProbeExchange::default(),
        ));
        self.run(fingerprint, requirements, storage_writer, &mut prober)
    }

    fn decide_without_endpoint(
        &self,
        request: &AttachOrLaunchRequest,
        storage_writer: StorageWriterState,
        lock: LockAcquireOutcome,
    ) -> Result<AttachOrLaunchRunResult, AttachOrLaunchRunError> {
        let launch_lock = match &lock {
            LockAcquireOutcome::Acquired(_) => LaunchLockState::Acquired,
            LockAcquireOutcome::Busy { .. } => LaunchLockState::Busy,
        };
        Ok(
            match AttachOrLaunchDecider::decide(request, None, None, launch_lock, storage_writer) {
                AttachOrLaunchDecision::LaunchNew => match lock {
                    LockAcquireOutcome::Acquired(lock) => {
                        AttachOrLaunchRunResult::LaunchNew { lock }
                    }
                    LockAcquireOutcome::Busy { .. } => {
                        return Err(AttachOrLaunchRunError::InvariantViolation {
                            message: "decider requested launch while launch lock is busy"
                                .to_string(),
                        });
                    }
                },
                AttachOrLaunchDecision::WaitForLaunch => match lock {
                    LockAcquireOutcome::Busy { path } => {
                        AttachOrLaunchRunResult::WaitForLaunch { lock_path: path }
                    }
                    LockAcquireOutcome::Acquired(_) => {
                        return Err(AttachOrLaunchRunError::InvariantViolation {
                            message: "decider requested wait while launch lock is acquired"
                                .to_string(),
                        });
                    }
                },
                AttachOrLaunchDecision::Fail { reason } => AttachOrLaunchRunResult::Fail { reason },
                unexpected => {
                    return Err(AttachOrLaunchRunError::InvariantViolation {
                        message: format!("unexpected no-endpoint decision: {unexpected:?}"),
                    });
                }
            },
        )
    }

    fn handle_endpoint_decision<P: EndpointProber>(
        &self,
        request: &AttachOrLaunchRequest,
        fingerprint: &StateRootFingerprint,
        prober: &mut P,
        decision: AttachOrLaunchDecision,
    ) -> Result<EndpointDecisionResult, AttachOrLaunchRunError> {
        match decision {
            AttachOrLaunchDecision::ProbeEndpoint {
                target,
                requirements,
            } => {
                let report = prober.probe(target.clone(), requirements.clone())?;
                if report.target != target || report.requirements != requirements {
                    return Err(AttachOrLaunchRunError::ProbeReportMismatch);
                }
                let endpoint = self.endpoint_records.read(fingerprint)?;
                let Some(endpoint) = endpoint.as_ref() else {
                    return Ok(EndpointDecisionResult::RetryAfterCleanup);
                };
                if !target_matches_record(&target, endpoint) {
                    return Ok(EndpointDecisionResult::RetryWithoutCleanup);
                }
                let next = AttachOrLaunchDecider::decide(
                    request,
                    Some(endpoint),
                    Some(&report),
                    LaunchLockState::Acquired,
                    StorageWriterState::Available,
                );
                if matches!(next, AttachOrLaunchDecision::ProbeEndpoint { .. }) {
                    return Err(AttachOrLaunchRunError::ProbeReportMismatch);
                }
                self.handle_endpoint_decision(request, fingerprint, prober, next)
            }
            AttachOrLaunchDecision::AttachExisting { target } => Ok(EndpointDecisionResult::Done(
                AttachOrLaunchRunResult::AttachExisting { target },
            )),
            AttachOrLaunchDecision::CleanStaleEndpoint { target, .. } => {
                if self.remove_if_current_target(fingerprint, &target)? {
                    Ok(EndpointDecisionResult::RetryAfterCleanup)
                } else {
                    Ok(EndpointDecisionResult::RetryWithoutCleanup)
                }
            }
            AttachOrLaunchDecision::Fail { reason } => Ok(EndpointDecisionResult::Done(
                AttachOrLaunchRunResult::Fail { reason },
            )),
            unexpected @ (AttachOrLaunchDecision::LaunchNew
            | AttachOrLaunchDecision::WaitForLaunch) => {
                Err(AttachOrLaunchRunError::InvariantViolation {
                    message: format!("unexpected endpoint decision: {unexpected:?}"),
                })
            }
        }
    }

    fn remove_if_current_target(
        &self,
        fingerprint: &StateRootFingerprint,
        stale_target: &EndpointTarget,
    ) -> Result<bool, EndpointRecordStoreError> {
        self.endpoint_records.remove_if(fingerprint, |current| {
            target_matches_record(stale_target, current)
        })
    }
}

#[derive(Debug)]
pub enum AttachOrLaunchRunResult {
    AttachExisting { target: EndpointTarget },
    LaunchNew { lock: RuntimeLock },
    WaitForLaunch { lock_path: PathBuf },
    Fail { reason: AttachOrLaunchFailure },
}

enum EndpointDecisionResult {
    Done(AttachOrLaunchRunResult),
    RetryAfterCleanup,
    RetryWithoutCleanup,
}

#[derive(Debug, Error)]
pub enum AttachOrLaunchRunError {
    #[error(transparent)]
    EndpointRecord(#[from] EndpointRecordStoreError),
    #[error(transparent)]
    LaunchLock(#[from] RuntimeLockError),
    #[error(transparent)]
    Probe(#[from] EndpointProbeError),
    #[error("attach-or-launch stale cleanup repeated without progress")]
    RepeatedStaleCleanup,
    #[error("endpoint probe report did not match the requested endpoint target")]
    ProbeReportMismatch,
    #[error("attach-or-launch invariant violation: {message}")]
    InvariantViolation { message: String },
}

fn target_matches_record(
    target: &EndpointTarget,
    record: &crate::storage_runtime::RuntimeEndpointRecord,
) -> bool {
    target.server_id == record.server_id
        && target.state_root_fingerprint == record.state_root_fingerprint
        && target.protocol_version == record.protocol_version
        && target.app_version == record.app_version
        && target.auth_token == record.auth_token
        && target.endpoints == record.endpoints
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
