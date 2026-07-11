use std::path::{Path, PathBuf};
use std::time::Duration;

use thiserror::Error;

use super::runner::{
    AttachOrLaunchRequirements, AttachOrLaunchRunError, AttachOrLaunchRunResult,
    AttachOrLaunchRunner,
};
use super::{AttachOrLaunchFailure, EndpointTarget, StorageWriterState};
use crate::storage_runtime::{RuntimeLock, StateRootFingerprint};

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
            max_wait_attempts: 20,
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
    #[error("App Server launch is still in progress: {lock_path}")]
    LaunchStillInProgress { lock_path: PathBuf },
}

#[cfg(test)]
mod tests;
