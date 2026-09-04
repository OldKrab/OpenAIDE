use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};

use openaide_app_server_protocol::snapshot::{
    AgentCapabilities, AgentSetupReason, AgentSignInFlow, AgentSignInPhase, AgentStatus,
};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentAuthMethodSummary, AgentProbeResult};

/// In-memory truth for one Agent's status and its Sign-in Flow. `sign_in` is the only
/// representation of a running or failed sign-in; clients never keep their own copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentStatusSnapshot {
    pub(crate) status: AgentStatus,
    pub(crate) setup_reason: Option<AgentSetupReason>,
    pub(crate) capabilities: AgentCapabilities,
    pub(crate) auth_methods: Vec<AgentAuthMethodSummary>,
    pub(crate) logout_supported: bool,
    pub(crate) sign_in: Option<AgentSignInFlow>,
    pub(crate) status_before_authentication: Option<AgentStatus>,
}

impl Default for AgentStatusSnapshot {
    fn default() -> Self {
        Self {
            status: AgentStatus::Disconnected,
            setup_reason: None,
            capabilities: AgentCapabilities::default(),
            auth_methods: Vec::new(),
            logout_supported: false,
            sign_in: None,
            status_before_authentication: None,
        }
    }
}

impl AgentStatusSnapshot {
    /// Method id of the flow currently running (not a failed leftover).
    pub(crate) fn running_sign_in_method_id(&self) -> Option<&str> {
        self.sign_in
            .as_ref()
            .filter(|flow| flow.phase != AgentSignInPhase::Failed)
            .map(|flow| flow.method_id.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AgentStatusCache {
    entries: Arc<Mutex<HashMap<String, AgentStatusSnapshot>>>,
    updates: Option<mpsc::Sender<()>>,
}

pub(crate) type AgentStatusUpdateReceiver = mpsc::Receiver<()>;

impl AgentStatusCache {
    pub(crate) fn channel() -> (Self, AgentStatusUpdateReceiver) {
        let (sender, receiver) = mpsc::channel();
        (
            Self {
                entries: Arc::default(),
                updates: Some(sender),
            },
            receiver,
        )
    }
    pub(crate) fn record_connected(&self, agent_id: &str) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        if snapshot.status == AgentStatus::Authenticating {
            return;
        }
        if snapshot.status == AgentStatus::Connected {
            return;
        }
        snapshot.status = AgentStatus::Connected;
        snapshot.setup_reason = None;
        drop(entries);
        self.notify();
    }

    /// Publishes the shared prerequisite without treating it as Agent process launch.
    pub(crate) fn begin_installation(&self, agent_id: &str) -> AgentStatus {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        let previous = snapshot.status;
        snapshot.status = AgentStatus::Installing;
        drop(entries);
        self.notify();
        previous
    }

    pub(crate) fn complete_installation(&self, agent_id: &str, previous: AgentStatus) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        snapshot.status = if previous == AgentStatus::Authenticating {
            AgentStatus::Authenticating
        } else {
            AgentStatus::Launching
        };
        drop(entries);
        self.notify();
    }

    pub(crate) fn record_launching(&self, agent_id: &str) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        if snapshot.status != AgentStatus::Authenticating {
            snapshot.status = AgentStatus::Launching;
        }
        drop(entries);
        self.notify();
    }

    pub(crate) fn record_session_error(&self, agent_id: &str, error: &RuntimeError) {
        if !session_error_updates_agent_status(error) {
            return;
        }
        self.record_probe_error(agent_id, error);
    }

    pub(crate) fn record_probe_success(&self, result: &AgentProbeResult) {
        self.record(
            result.agent_id.clone(),
            AgentStatusSnapshot {
                status: AgentStatus::Connected,
                setup_reason: None,
                capabilities: capabilities_from_probe(result),
                auth_methods: result.auth_methods.clone(),
                logout_supported: result.logout_supported,
                sign_in: None,
                status_before_authentication: None,
            },
        );
    }

    pub(crate) fn record_probe_error(&self, agent_id: &str, error: &RuntimeError) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let previous = entries.get(agent_id).cloned().unwrap_or_default();
        entries.insert(
            agent_id.to_string(),
            AgentStatusSnapshot {
                status: status_from_probe_error(error),
                setup_reason: setup_reason_from_probe_error(error),
                capabilities: AgentCapabilities::default(),
                // ACP Authentication Methods are available choices, not proof of current
                // authentication. Keep the last advertised choices actionable during recovery.
                auth_methods: previous.auth_methods,
                logout_supported: previous.logout_supported,
                // A probe failure explains the Agent, not the user's last sign-in attempt.
                sign_in: previous
                    .sign_in
                    .filter(|flow| flow.phase == AgentSignInPhase::Failed),
                status_before_authentication: None,
            },
        );
        drop(entries);
        self.notify();
    }

    pub(crate) fn snapshot(&self, agent_id: &str) -> AgentStatusSnapshot {
        self.entries
            .lock()
            .expect("agent status cache poisoned")
            .get(agent_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn begin_authentication(
        &self,
        agent_id: &str,
        method_id: &str,
        continuing: bool,
    ) -> Result<(), RuntimeError> {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        if snapshot.status == AgentStatus::Authenticating {
            if continuing && snapshot.running_sign_in_method_id() == Some(method_id) {
                return Ok(());
            }
            return Err(RuntimeError::Conflict(format!(
                "Authentication is already in progress for Agent {agent_id}"
            )));
        }
        snapshot.status_before_authentication = Some(snapshot.status);
        snapshot.status = AgentStatus::Authenticating;
        snapshot.sign_in = Some(AgentSignInFlow {
            method_id: method_id.to_string(),
            phase: AgentSignInPhase::Starting,
            url: None,
            hint: None,
            failure: None,
        });
        drop(entries);
        self.notify();
        Ok(())
    }

    /// The Agent asked the user to open `url` (with an optional hint such as a device code).
    /// Ignored unless a flow is running: a late URL from a cancelled flow must not resurrect it.
    pub(crate) fn record_sign_in_awaiting_user(
        &self,
        agent_id: &str,
        url: String,
        hint: Option<String>,
    ) -> bool {
        self.update_running_flow(agent_id, |flow| {
            flow.phase = AgentSignInPhase::AwaitingUser;
            flow.url = Some(url);
            flow.hint = hint;
        })
    }

    /// A terminal-kind method opened its terminal and waits for the user's confirmation.
    pub(crate) fn record_sign_in_awaiting_terminal(&self, agent_id: &str) -> bool {
        self.update_running_flow(agent_id, |flow| {
            flow.phase = AgentSignInPhase::AwaitingTerminal;
        })
    }

    pub(crate) fn record_authentication_success(&self, agent_id: &str) {
        self.update_authentication_result(agent_id, AgentStatus::Connected);
    }

    pub(crate) fn record_logout_success(&self, agent_id: &str) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        snapshot.status = AgentStatus::AuthRequired;
        snapshot.sign_in = None;
        snapshot.status_before_authentication = None;
        drop(entries);
        self.notify();
    }

    /// Ends the running flow. `failure` keeps a product-safe summary visible until the user
    /// starts another flow or dismisses it; `None` (cancellation) leaves no trace.
    pub(crate) fn record_authentication_error(
        &self,
        agent_id: &str,
        error: &RuntimeError,
        failure: Option<String>,
    ) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        if snapshot.status != AgentStatus::Authenticating {
            return;
        }
        snapshot.status = snapshot
            .status_before_authentication
            .take()
            .unwrap_or_else(|| status_from_probe_error(error));
        snapshot.sign_in = match (snapshot.sign_in.take(), failure) {
            (Some(flow), Some(failure)) => Some(AgentSignInFlow {
                method_id: flow.method_id,
                phase: AgentSignInPhase::Failed,
                url: None,
                hint: None,
                failure: Some(failure),
            }),
            _ => None,
        };
        drop(entries);
        self.notify();
    }

    /// Removes a failed flow the user acknowledged. No-op while a flow is running.
    pub(crate) fn dismiss_failed_sign_in(&self, agent_id: &str) -> bool {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let Some(snapshot) = entries.get_mut(agent_id) else {
            return false;
        };
        if snapshot.status == AgentStatus::Authenticating
            || snapshot
                .sign_in
                .as_ref()
                .is_none_or(|flow| flow.phase != AgentSignInPhase::Failed)
        {
            return false;
        }
        snapshot.sign_in = None;
        drop(entries);
        self.notify();
        true
    }

    fn update_running_flow(
        &self,
        agent_id: &str,
        update: impl FnOnce(&mut AgentSignInFlow),
    ) -> bool {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let Some(snapshot) = entries.get_mut(agent_id) else {
            return false;
        };
        if snapshot.status != AgentStatus::Authenticating {
            return false;
        }
        let Some(flow) = snapshot.sign_in.as_mut() else {
            return false;
        };
        update(flow);
        drop(entries);
        self.notify();
        true
    }

    pub(crate) fn clear(&self, agent_id: &str) -> bool {
        self.entries
            .lock()
            .expect("agent status cache poisoned")
            .remove(agent_id)
            .is_some()
    }

    fn record(&self, agent_id: String, snapshot: AgentStatusSnapshot) {
        self.entries
            .lock()
            .expect("agent status cache poisoned")
            .insert(agent_id, snapshot);
        self.notify();
    }

    fn notify(&self) {
        if let Some(updates) = &self.updates {
            let _ = updates.send(());
        }
    }

    fn update_authentication_result(&self, agent_id: &str, status: AgentStatus) {
        let mut entries = self.entries.lock().expect("agent status cache poisoned");
        let snapshot = entries.entry(agent_id.to_string()).or_default();
        snapshot.status = status;
        snapshot.setup_reason = None;
        snapshot.sign_in = None;
        snapshot.status_before_authentication = None;
        drop(entries);
        self.notify();
    }

    #[cfg(test)]
    pub(crate) fn record_for_test(&self, agent_id: String, snapshot: AgentStatusSnapshot) {
        self.record(agent_id, snapshot);
    }
}

fn capabilities_from_probe(result: &AgentProbeResult) -> AgentCapabilities {
    AgentCapabilities {
        resume_tasks: result.typed_capabilities.resume_sessions,
        delete_native_sessions: result.typed_capabilities.delete_sessions,
        fork_native_sessions: result.typed_capabilities.fork_sessions,
    }
}

fn status_from_probe_error(error: &RuntimeError) -> AgentStatus {
    match error {
        RuntimeError::AuthRequired(_) => AgentStatus::AuthRequired,
        RuntimeError::SetupRequired(_) | RuntimeError::NodeJsRequired(_) => {
            AgentStatus::SetupRequired
        }
        RuntimeError::Unsupported(_) => AgentStatus::Unsupported,
        RuntimeError::CapabilityMissing(_) | RuntimeError::MethodNotFound(_) => {
            AgentStatus::Unsupported
        }
        RuntimeError::NotReady(_)
        | RuntimeError::Internal(_)
        | RuntimeError::InvalidParams(_)
        | RuntimeError::TaskNotFound(_)
        | RuntimeError::Storage(_)
        | RuntimeError::Conflict(_) => AgentStatus::Failed,
    }
}

fn setup_reason_from_probe_error(error: &RuntimeError) -> Option<AgentSetupReason> {
    matches!(error, RuntimeError::NodeJsRequired(_)).then_some(AgentSetupReason::NodeJsRequired)
}

fn session_error_updates_agent_status(error: &RuntimeError) -> bool {
    matches!(
        error,
        RuntimeError::AuthRequired(_)
            | RuntimeError::SetupRequired(_)
            | RuntimeError::NodeJsRequired(_)
            | RuntimeError::Unsupported(_)
            | RuntimeError::CapabilityMissing(_)
            | RuntimeError::MethodNotFound(_)
            | RuntimeError::NotReady(_)
            | RuntimeError::Internal(_)
    )
}

#[cfg(test)]
#[path = "status_cache_tests.rs"]
mod tests;
