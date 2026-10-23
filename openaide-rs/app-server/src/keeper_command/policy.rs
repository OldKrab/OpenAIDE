//! When the keeper holds the App Server on an SSH Computer and when it leaves
//! (ADR-0069). The keeper is an ordinary client; leaving lets the App Server stop
//! by its own rule once no other client remains.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeeperPolicy {
    /// How long after the last App Shell left the keeper still holds an idle App Server.
    pub(crate) app_shell_grace: Duration,
    /// How long Tasks waiting for the user keep holding it with nobody attached.
    pub(crate) waiting_limit: Duration,
}

impl Default for KeeperPolicy {
    fn default() -> Self {
        Self {
            app_shell_grace: Duration::from_secs(30 * 60),
            waiting_limit: Duration::from_secs(24 * 60 * 60),
        }
    }
}

/// What the keeper read from `client/replacementImpact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Observation {
    pub(crate) active_turns: u32,
    pub(crate) pending_task_requests: u32,
    /// Attached clients that are App Shells, not other keepers or `remote` commands.
    pub(crate) app_shells: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Hold(HoldReason),
    Leave(LeaveReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoldReason {
    RunningWork,
    AppShellAttached,
    WaitingForUser,
    RecentAppShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeaveReason {
    Idle,
    WaitedTooLong,
}

impl HoldReason {
    pub(crate) fn class(self) -> &'static str {
        match self {
            Self::RunningWork => "running_work",
            Self::AppShellAttached => "app_shell_attached",
            Self::WaitingForUser => "waiting_for_user",
            Self::RecentAppShell => "recent_app_shell",
        }
    }
}

impl LeaveReason {
    pub(crate) fn class(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::WaitedTooLong => "waited_too_long",
        }
    }
}

/// The keeper's memory between observations. Waiting time is measured by the keeper
/// from the first observation that saw a waiting Task, so it is never longer than
/// the real wait.
#[derive(Debug, Clone)]
pub(crate) struct KeeperClock {
    policy: KeeperPolicy,
    last_app_shell: Instant,
    waiting_since: Option<Instant>,
}

impl KeeperClock {
    /// The keeper starts because an App Shell is connecting, so that counts as one
    /// just seen.
    pub(crate) fn new(policy: KeeperPolicy, started: Instant) -> Self {
        Self {
            policy,
            last_app_shell: started,
            waiting_since: None,
        }
    }

    pub(crate) fn observe(&mut self, observation: &Observation, now: Instant) -> Verdict {
        if observation.app_shells > 0 {
            self.last_app_shell = now;
        }
        if observation.pending_task_requests == 0 {
            self.waiting_since = None;
        } else if self.waiting_since.is_none() {
            self.waiting_since = Some(now);
        }

        if observation.active_turns > 0 {
            return Verdict::Hold(HoldReason::RunningWork);
        }
        if observation.app_shells > 0 {
            return Verdict::Hold(HoldReason::AppShellAttached);
        }
        if let Some(since) = self.waiting_since {
            if now.duration_since(since) < self.policy.waiting_limit {
                return Verdict::Hold(HoldReason::WaitingForUser);
            }
        }
        if now.duration_since(self.last_app_shell) < self.policy.app_shell_grace {
            return Verdict::Hold(HoldReason::RecentAppShell);
        }
        Verdict::Leave(match self.waiting_since {
            Some(_) => LeaveReason::WaitedTooLong,
            None => LeaveReason::Idle,
        })
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
