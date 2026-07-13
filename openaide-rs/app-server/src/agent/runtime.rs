use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::watch;

use crate::agent::events::{AgentEvent, AgentPermissionOutcome, AgentPermissionRequest};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentCommandsCatalog, AgentListSessionsResult, AgentProbeResult,
    Attachment, ConfigOptionsCatalog, NormalizedMessage,
};

/// Identifies a Native Session within the Agent that owns its identifier.
///
/// ACP only guarantees `session_id` uniqueness inside one Agent. Runtime
/// registries and lifecycle operations must therefore carry both fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AgentSessionKey {
    agent_id: String,
    session_id: String,
}

impl AgentSessionKey {
    pub fn new(agent_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            session_id: session_id.into(),
        }
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

#[derive(Clone)]
pub struct AgentSession {
    pub agent_id: String,
    pub session_id: String,
    pub config_options: HashMap<String, String>,
    pub config_catalog: Option<ConfigOptionsCatalog>,
    pub commands_catalog: Option<AgentCommandsCatalog>,
    pub model_id: Option<String>,
}

impl AgentSession {
    pub fn new(agent_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            session_id: session_id.into(),
            config_options: HashMap::new(),
            config_catalog: None,
            commands_catalog: None,
            model_id: None,
        }
    }

    pub fn key(&self) -> AgentSessionKey {
        AgentSessionKey::new(self.agent_id.clone(), self.session_id.clone())
    }

    pub fn with_config_options(mut self, catalog: &ConfigOptionsCatalog) -> Self {
        self.config_options = catalog.current_values();
        self.model_id = catalog.model_id();
        self.config_catalog = Some(catalog.clone());
        self
    }

    pub fn with_commands_catalog(mut self, catalog: Option<AgentCommandsCatalog>) -> Self {
        self.commands_catalog = catalog;
        self
    }
}

#[derive(Clone)]
pub struct AgentLoadedSession {
    pub session: AgentSession,
    pub replayed_messages: Vec<NormalizedMessage>,
}

/// Semantic result of one primary ACP prompt request.
///
/// Session updates have an independent lifetime; this value only explains why
/// the Agent stopped working on the request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentPromptOutcome {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
    Other(String),
}

pub trait AgentSecretResolver: Send + Sync {
    fn resolve_secret_env(
        &self,
        agent_id: &str,
        names: &[String],
    ) -> Result<HashMap<String, String>, RuntimeError>;
}

#[derive(Clone)]
pub struct AgentSessionStart {
    pub agent_id: String,
    pub task_id: String,
    pub cwd: String,
    pub model_id: Option<String>,
    pub config_options: Option<Value>,
    pub config_option_policy: ConfigOptionPolicy,
    pub context: Vec<Attachment>,
    pub cancellation: TurnCancellation,
    pub secret_resolver: Option<Arc<dyn AgentSecretResolver>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConfigOptionPolicy {
    /// Reject selections that are absent from the Agent's fresh catalog.
    #[default]
    Strict,
    /// Keep fresh Agent defaults when persisted draft selections are stale.
    ReconcileWithAgentDefaults,
}

#[derive(Clone)]
pub struct AgentSessionResume {
    pub agent_id: String,
    pub task_id: String,
    pub session_id: String,
    pub cwd: String,
    pub model_id: Option<String>,
    pub cancellation: TurnCancellation,
}

impl AgentSessionResume {
    pub fn session_key(&self) -> AgentSessionKey {
        AgentSessionKey::new(self.agent_id.clone(), self.session_id.clone())
    }
}

#[derive(Clone)]
pub struct AgentSessionLoad {
    pub agent_id: String,
    pub task_id: String,
    pub session_id: String,
    pub cwd: String,
    pub model_id: Option<String>,
    pub cancellation: TurnCancellation,
    pub secret_resolver: Option<Arc<dyn AgentSecretResolver>>,
}

impl AgentSessionLoad {
    pub fn session_key(&self) -> AgentSessionKey {
        AgentSessionKey::new(self.agent_id.clone(), self.session_id.clone())
    }
}

#[derive(Clone)]
pub struct AgentPrompt {
    pub agent_id: String,
    pub task_id: String,
    pub session_id: String,
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub cancellation: TurnCancellation,
}

impl AgentPrompt {
    pub fn session_key(&self) -> AgentSessionKey {
        AgentSessionKey::new(self.agent_id.clone(), self.session_id.clone())
    }
}

#[derive(Clone)]
pub struct AgentSessionSetConfigOptionRequest {
    pub agent_id: String,
    pub session_id: String,
    pub config_id: String,
    pub value: String,
}

impl AgentSessionSetConfigOptionRequest {
    pub fn session_key(&self) -> AgentSessionKey {
        AgentSessionKey::new(self.agent_id.clone(), self.session_id.clone())
    }
}

#[derive(Clone)]
pub struct AgentProbeRequest {
    pub agent_id: String,
}

#[derive(Clone)]
pub struct AgentAuthenticateRequest {
    pub agent_id: String,
    pub method_id: String,
}

#[derive(Clone)]
pub struct AgentListSessionsRequest {
    pub agent_id: String,
    pub cwd: String,
    pub cursor: Option<String>,
}

#[derive(Clone)]
pub struct AgentSessionDelete {
    pub agent_id: String,
    pub session_id: String,
}

impl AgentSessionDelete {
    pub fn session_key(&self) -> AgentSessionKey {
        AgentSessionKey::new(self.agent_id.clone(), self.session_id.clone())
    }
}

#[derive(Clone)]
pub struct TurnCancellation {
    cancelled: watch::Sender<bool>,
}

impl Default for TurnCancellation {
    fn default() -> Self {
        let (cancelled, _receiver) = watch::channel(false);
        Self { cancelled }
    }
}

impl TurnCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    /// Waits on the cancellation edge without adding polling latency.
    pub(crate) async fn cancelled(&self) {
        let mut cancelled = self.cancelled.subscribe();
        if *cancelled.borrow() {
            return;
        }
        while cancelled.changed().await.is_ok() {
            if *cancelled.borrow_and_update() {
                return;
            }
        }
    }
}

pub trait AgentRuntime: Send + Sync {
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(format!(
            "agent_probe:{}",
            request.agent_id
        )))
    }

    fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(format!(
            "agent_authenticate:{}:{}",
            request.agent_id, request.method_id
        )))
    }

    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(format!(
            "agent_list_sessions:{}:{}",
            request.agent_id, request.cwd
        )))
    }

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError>;

    fn load_session(&self, request: AgentSessionLoad) -> Result<AgentLoadedSession, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(format!(
            "agent_load_session:{}:{}",
            request.task_id, request.session_id
        )))
    }

    fn set_session_config_option(
        &self,
        request: AgentSessionSetConfigOptionRequest,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        Err(RuntimeError::CapabilityMissing(format!(
            "agent_session_config_options:{}:{}:{}",
            request.agent_id, request.session_id, request.config_id
        )))
    }

    fn resume_session(&self, request: AgentSessionResume) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(request.agent_id, request.session_id))
    }

    fn attach_session_event_sink(
        &self,
        _session: &AgentSessionKey,
        _sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<AgentPromptOutcome, RuntimeError>;

    /// Sends an additional prompt to a Native Session without owning Task status.
    ///
    /// The runtime must consume any eventual protocol response, but the response
    /// cannot finish or otherwise change the primary prompt lifecycle.
    fn steer(&self, _prompt: AgentPrompt) -> Result<(), RuntimeError> {
        Err(RuntimeError::CapabilityMissing(
            "agent_session_steering".to_string(),
        ))
    }

    fn cancel_session(&self, _session: &AgentSessionKey) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn close_session(&self, _session: &AgentSessionKey) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn delete_session(&self, _request: AgentSessionDelete) -> Result<(), RuntimeError> {
        Err(RuntimeError::CapabilityMissing(
            "agent_session_delete".to_string(),
        ))
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

pub trait AgentEventSink: Send + Sync {
    fn emit(&self, event: AgentEvent) -> Result<(), RuntimeError>;

    fn request_permission(
        &self,
        request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError>;
}

/// A partial Agent-owned metadata field. ACP distinguishes omission from an
/// explicit clear, so runtime integrations must preserve all three states.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AgentMetadataField<T> {
    #[default]
    Unchanged,
    Clear,
    Value(T),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentSessionMetadataUpdate {
    pub title: AgentMetadataField<String>,
    pub updated_at: AgentMetadataField<String>,
}

pub trait AgentSessionEventSink: Send + Sync {
    /// Receives normalized session/update content independently from any prompt request.
    fn session_update(&self, _event: AgentEvent) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn config_options_changed(&self, catalog: ConfigOptionsCatalog) -> Result<(), RuntimeError>;

    fn commands_changed(&self, catalog: AgentCommandsCatalog) -> Result<(), RuntimeError>;

    fn metadata_changed(&self, _update: AgentSessionMetadataUpdate) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Presents an Agent-owned session question, including while no prompt turn is active.
    fn request_question(
        &self,
        _form: openaide_app_server_protocol::server_requests::QuestionRequestParams,
        _cancellation: TurnCancellation,
    ) -> Result<openaide_app_server_protocol::server_requests::QuestionRequestResponse, RuntimeError>
    {
        Ok(openaide_app_server_protocol::server_requests::QuestionRequestResponse::Cancel)
    }

    fn record_question_error(&self, _message: String) -> Result<(), RuntimeError> {
        Ok(())
    }
}
