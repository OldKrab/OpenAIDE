use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

use crate::agent::events::{AgentEvent, AgentPermissionOutcome, AgentPermissionRequest};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentCommandsCatalog, AgentListSessionsResult, AgentProbeResult,
    Attachment, ConfigOptionsCatalog, NormalizedMessage,
};

#[derive(Clone)]
pub struct AgentSession {
    pub session_id: String,
    pub config_options: HashMap<String, String>,
    pub config_catalog: Option<ConfigOptionsCatalog>,
    pub commands_catalog: Option<AgentCommandsCatalog>,
    pub model_id: Option<String>,
}

impl AgentSession {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            config_options: HashMap::new(),
            config_catalog: None,
            commands_catalog: None,
            model_id: None,
        }
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
    pub context: Vec<Attachment>,
    pub cancellation: TurnCancellation,
    pub secret_resolver: Option<Arc<dyn AgentSecretResolver>>,
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

#[derive(Clone)]
pub struct AgentPrompt {
    pub task_id: String,
    pub session_id: String,
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub cancellation: TurnCancellation,
}

#[derive(Clone)]
pub struct AgentSessionSetConfigOptionRequest {
    pub agent_id: String,
    pub session_id: String,
    pub config_id: String,
    pub value: String,
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
    pub session_id: String,
}

#[derive(Clone, Default)]
pub struct TurnCancellation {
    cancelled: Arc<AtomicBool>,
}

impl TurnCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
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
        Ok(AgentSession::new(request.session_id))
    }

    fn attach_session_event_sink(
        &self,
        _session_id: &str,
        _sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError>;

    fn cancel_session(&self, _session_id: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn close_session(&self, _session_id: &str) -> Result<(), RuntimeError> {
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

pub trait AgentEventSink: Send + Sync {
    fn emit(&self, event: AgentEvent) -> Result<(), RuntimeError>;

    /// Finalizes output owned by the current Agent prompt before another prompt takes over.
    fn finish_prompt(&self) -> Result<(), RuntimeError> {
        Ok(())
    }

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
