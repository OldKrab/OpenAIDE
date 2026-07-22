use std::path::{Path, PathBuf};
use std::time::Duration;

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
}

impl Default for LaunchHandoffPolicy {
    fn default() -> Self {
        Self {
            // Startup and authenticated replacement share one bounded 50-second
            // window, below the shell adapters' final 60-second safety ceiling.
            max_wait_attempts: 200,
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
                AttachOrLaunchRunResult::ReplaceIncompatible { target, reason } => {
                    let replacement = request_local_http_replacement(&target)?;
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

#[derive(Debug)]
pub enum LaunchHandoffResult {
    AttachExisting { target: EndpointTarget },
    LaunchNew { lock: RuntimeLock },
    Fail { reason: AttachOrLaunchFailure },
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
