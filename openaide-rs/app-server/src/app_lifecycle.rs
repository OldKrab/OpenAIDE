use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};

use crate::client_lifecycle::AppServerTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    Running,
    Draining,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitializeAdmission {
    Accepted,
    AcceptedAndAbortedDraining,
    Rejected(ProtocolError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientLifecycleEffect {
    Noop,
    BeginDraining,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownRequestOutcome {
    AlreadyStopping,
    ShutdownPlanned(ShutdownPlan),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownPlan {
    pub stop_accepting_new_work: bool,
    pub interrupt_pending_requests: bool,
    pub detach_agent_transports: bool,
    pub remove_endpoint_records: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownCompletion {
    CleanRelease,
    UncleanLeaseExpiryRequired,
}

#[derive(Debug, Clone)]
pub struct AppLifecycle {
    state: LifecycleState,
}

impl AppLifecycle {
    pub fn new() -> Self {
        Self {
            state: LifecycleState::Running,
        }
    }

    pub fn state(&self) -> LifecycleState {
        self.state
    }

    pub fn admit_initialize(&mut self, _now: AppServerTime) -> InitializeAdmission {
        match self.state {
            LifecycleState::Running => InitializeAdmission::Accepted,
            LifecycleState::Draining => {
                self.state = LifecycleState::Running;
                InitializeAdmission::AcceptedAndAbortedDraining
            }
            LifecycleState::Stopping => InitializeAdmission::Rejected(ProtocolError {
                code: ProtocolErrorCode::ServerStopping,
                message: "App Server is stopping".to_string(),
                recoverable: true,
                target: None,
            }),
        }
    }

    pub fn begin_draining(&mut self) {
        if self.state == LifecycleState::Running {
            self.state = LifecycleState::Draining;
        }
    }

    pub fn observe_last_client_expired(&mut self) -> ClientLifecycleEffect {
        match self.state {
            LifecycleState::Running => {
                self.state = LifecycleState::Draining;
                ClientLifecycleEffect::BeginDraining
            }
            LifecycleState::Draining | LifecycleState::Stopping => ClientLifecycleEffect::Noop,
        }
    }

    pub fn begin_stopping(&mut self) {
        self.state = LifecycleState::Stopping;
    }

    pub fn request_shutdown(&mut self) -> ShutdownRequestOutcome {
        if self.state == LifecycleState::Stopping {
            return ShutdownRequestOutcome::AlreadyStopping;
        }
        self.state = LifecycleState::Stopping;
        ShutdownRequestOutcome::ShutdownPlanned(ShutdownPlan {
            stop_accepting_new_work: true,
            interrupt_pending_requests: true,
            detach_agent_transports: true,
            remove_endpoint_records: true,
        })
    }

    pub fn complete_shutdown(&self, coherent_persistence: bool) -> ShutdownCompletion {
        if coherent_persistence {
            ShutdownCompletion::CleanRelease
        } else {
            ShutdownCompletion::UncleanLeaseExpiryRequired
        }
    }
}

impl Default for AppLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
