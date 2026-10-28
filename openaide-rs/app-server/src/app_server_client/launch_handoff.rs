use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use thiserror::Error;

use super::replacement::{request_local_http_replacement, ReplacementRequestOutcome};
use super::runner::{
    AttachOrLaunchRequirements, AttachOrLaunchRunError, AttachOrLaunchRunResult,
    AttachOrLaunchRunner,
};
use super::{AttachOrLaunchFailure, EndpointTarget, StorageWriterState};
use crate::storage_runtime::{RuntimeLock, StateRootFingerprint};

// The release immediately before authenticated replacement support needs its existing
// 30-second heartbeat timeout plus 10-second reconnect grace to stop without a PID kill.
const LEGACY_REPLACEMENT_MAX_WAIT_ATTEMPTS: usize = 200;

pub trait LaunchWaiter {
    fn wait_for_launch_progress(&mut self, lock_path: &Path) -> Result<(), LaunchWaitError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("launch wait failed: {message}")]
pub struct LaunchWaitError {
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SleepLaunchWaiter {
    delay: Duration,
}

impl SleepLaunchWaiter {
    pub fn new(delay: Duration) -> Self {
        Self { delay }
    }
}

impl Default for SleepLaunchWaiter {
    fn default() -> Self {
        Self {
            delay: Duration::from_millis(250),
        }
    }
}

impl LaunchWaiter for SleepLaunchWaiter {
    fn wait_for_launch_progress(&mut self, _lock_path: &Path) -> Result<(), LaunchWaitError> {
        std::thread::sleep(self.delay);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchHandoffPolicy {
    pub max_wait_attempts: usize,
    pub incompatible: IncompatibleServerPolicy,
}

/// What to do with a running App Server of another version for the same state root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncompatibleServerPolicy {
    /// Ask it to stop and start this version. Same-machine App Shells are installed
    /// with their App Server, so the newest one installed wins.
    Replace,
    /// Return it untouched, for a caller that must ask the user first (ADR-0069).
    Report,
}

impl Default for LaunchHandoffPolicy {
    fn default() -> Self {
        Self {
            // Startup and authenticated replacement share one bounded 50-second
            // window, below the shell adapters' final 60-second safety ceiling.
            max_wait_attempts: 200,
            incompatible: IncompatibleServerPolicy::Replace,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AttachOrLaunchHandoff {
    runner: AttachOrLaunchRunner,
    policy: LaunchHandoffPolicy,
}

impl AttachOrLaunchHandoff {
    pub fn new(runner: AttachOrLaunchRunner, policy: LaunchHandoffPolicy) -> Self {
        Self { runner, policy }
    }

    pub fn run<W: LaunchWaiter>(
        &self,
        fingerprint: &StateRootFingerprint,
        requirements: &AttachOrLaunchRequirements,
        storage_writer: StorageWriterState,
        waiter: &mut W,
    ) -> Result<LaunchHandoffResult, LaunchHandoffError> {
        let mut replacement = None;
        let result = self.run_handoff(
            fingerprint,
            requirements,
            storage_writer,
            waiter,
            &mut replacement,
        );
        if let Some(replacement) = replacement {
            replacement.finish(&result);
        }
        result
    }

    fn run_handoff<W: LaunchWaiter>(
        &self,
        fingerprint: &StateRootFingerprint,
        requirements: &AttachOrLaunchRequirements,
        storage_writer: StorageWriterState,
        waiter: &mut W,
        replacement_log: &mut Option<ReplacementLog>,
    ) -> Result<LaunchHandoffResult, LaunchHandoffError> {
        let mut wait_attempts = 0;
        loop {
            match self.runner.run_with_local_transports(
                fingerprint,
                requirements,
                storage_writer,
            )? {
                AttachOrLaunchRunResult::AttachExisting { target } => {
                    return Ok(LaunchHandoffResult::AttachExisting { target });
                }
                AttachOrLaunchRunResult::LaunchNew { lock } => {
                    return Ok(LaunchHandoffResult::LaunchNew { lock });
                }
                AttachOrLaunchRunResult::Fail { reason } => {
                    return Ok(LaunchHandoffResult::Fail { reason });
                }
                AttachOrLaunchRunResult::ReplaceIncompatible { target, reason }
                    if self.policy.incompatible == IncompatibleServerPolicy::Report =>
                {
                    return Ok(LaunchHandoffResult::Incompatible { target, reason });
                }
                AttachOrLaunchRunResult::ReplaceIncompatible { target, reason } => {
                    let replacement = request_local_http_replacement(&target);
                    // The loop re-requests on every wait; only the first is logged.
                    replacement_log
                        .get_or_insert_with(|| {
                            ReplacementLog::requested(&target, reason, requirements, &replacement)
                        })
                        .requests += 1;
                    let replacement = replacement?;
                    let max_wait_attempts = match replacement {
                        ReplacementRequestOutcome::AwaitLegacyShutdown => {
                            LEGACY_REPLACEMENT_MAX_WAIT_ATTEMPTS
                        }
                        ReplacementRequestOutcome::Accepted
                        | ReplacementRequestOutcome::Unreachable => self.policy.max_wait_attempts,
                    };
                    if wait_attempts >= max_wait_attempts {
                        return Err(LaunchHandoffError::ReplacementStillStopping {
                            server_id: target.server_id,
                            reason,
                        });
                    }
                    waiter.wait_for_launch_progress(self.runner.launch_lock_path())?;
                    wait_attempts += 1;
                }
                AttachOrLaunchRunResult::WaitForLaunch { lock_path } => {
                    if wait_attempts >= self.policy.max_wait_attempts {
                        return Err(LaunchHandoffError::LaunchStillInProgress { lock_path });
                    }
                    waiter.wait_for_launch_progress(&lock_path)?;
                    wait_attempts += 1;
                }
            }
        }
    }
}

/// One takeover of an incompatible App Server, from its first request to the handoff result.
///
/// The App Server being replaced only records that it was asked to stop, so this is
/// the one record of which version replaced it and whether the replacement then started.
struct ReplacementLog {
    started: Instant,
    server_id: String,
    requests: usize,
}

impl ReplacementLog {
    fn requested(
        target: &EndpointTarget,
        reason: AttachOrLaunchFailure,
        requirements: &AttachOrLaunchRequirements,
        outcome: &Result<ReplacementRequestOutcome, super::replacement::ReplacementRequestError>,
    ) -> Self {
        crate::logging::warn(
            "app_server_replacement_requested",
            serde_json::json!({
                "server_id": target.server_id,
                "reason_code": failure_reason_code(reason),
                "running_protocol_version": target.protocol_version,
                "running_app_version": target.app_version,
                "required_protocol_version": requirements.required_protocol_version,
                "required_app_version": requirements.required_app_version,
                "request_outcome": match outcome {
                    Ok(ReplacementRequestOutcome::Accepted) => "accepted",
                    Ok(ReplacementRequestOutcome::AwaitLegacyShutdown) => "await_legacy_shutdown",
                    Ok(ReplacementRequestOutcome::Unreachable) => "unreachable",
                    Err(_) => "request_failed",
                },
            }),
        );
        Self {
            started: Instant::now(),
            server_id: target.server_id.clone(),
            requests: 0,
        }
    }

    fn finish(self, result: &Result<LaunchHandoffResult, LaunchHandoffError>) {
        let outcome = match result {
            Ok(LaunchHandoffResult::LaunchNew { .. }) => "launch_new",
            Ok(LaunchHandoffResult::AttachExisting { .. }) => "attach_existing",
            Ok(LaunchHandoffResult::Fail { .. }) => "failed",
            Ok(LaunchHandoffResult::Incompatible { .. }) => "incompatible",
            Err(LaunchHandoffError::ReplacementStillStopping { .. }) => "still_stopping",
            Err(LaunchHandoffError::Replacement(_)) => "request_failed",
            Err(_) => "handoff_error",
        };
        let fields = serde_json::json!({
            "server_id": self.server_id,
            "outcome": outcome,
            "request_count": self.requests,
            "duration_ms": self.started.elapsed().as_millis(),
        });
        if outcome == "launch_new" {
            crate::logging::info("app_server_replacement_finished", fields);
        } else {
            crate::logging::warn("app_server_replacement_finished", fields);
        }
    }
}

fn failure_reason_code(reason: AttachOrLaunchFailure) -> &'static str {
    match reason {
        AttachOrLaunchFailure::IncompatibleProtocol => "incompatible_protocol",
        AttachOrLaunchFailure::IncompatibleApp => "incompatible_app",
        AttachOrLaunchFailure::AuthOrPermissionFailure => "auth_or_permission_failure",
        AttachOrLaunchFailure::StorageStateRootBlocked => "storage_state_root_blocked",
    }
}

#[derive(Debug)]
pub enum LaunchHandoffResult {
    AttachExisting {
        target: EndpointTarget,
    },
    LaunchNew {
        lock: RuntimeLock,
    },
    Fail {
        reason: AttachOrLaunchFailure,
    },
    /// Only under [`IncompatibleServerPolicy::Report`]: a live App Server of another
    /// version, left running.
    Incompatible {
        target: EndpointTarget,
        reason: AttachOrLaunchFailure,
    },
}

#[derive(Debug, Error)]
pub enum LaunchHandoffError {
    #[error(transparent)]
    AttachOrLaunch(#[from] AttachOrLaunchRunError),
    #[error(transparent)]
    Wait(#[from] LaunchWaitError),
    #[error(transparent)]
    Replacement(#[from] super::replacement::ReplacementRequestError),
    #[error("App Server launch is still in progress: {lock_path}")]
    LaunchStillInProgress { lock_path: PathBuf },
    #[error("incompatible App Server {server_id} is still stopping after replacement request ({reason:?})")]
    ReplacementStillStopping {
        server_id: String,
        reason: AttachOrLaunchFailure,
    },
}

#[cfg(test)]
#[path = "launch_handoff_tests.rs"]
mod tests;
