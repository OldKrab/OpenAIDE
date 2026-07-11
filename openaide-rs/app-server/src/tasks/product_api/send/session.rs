use openaide_app_server_protocol::errors::ProtocolError;

use crate::agent::{
    AgentSession, AgentSessionLoad, AgentSessionResume, AgentSessionStart, TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::storage::records::TaskRecord;
use crate::tasks::task_start_transaction::TaskSessionStartGuard;

use super::TaskProductApi;

pub(super) enum OpenedAgentSession<'a> {
    Started(TaskSessionStartGuard<'a>),
    Loaded(TaskSessionStartGuard<'a>),
    Resumed(AgentSession),
}

impl OpenedAgentSession<'_> {
    pub(super) fn session(&self) -> &AgentSession {
        match self {
            OpenedAgentSession::Started(guard) => guard.session(),
            OpenedAgentSession::Loaded(guard) => guard.session(),
            OpenedAgentSession::Resumed(session) => session,
        }
    }

    pub(super) fn commit(self) -> AgentSession {
        match self {
            OpenedAgentSession::Started(guard) => guard.commit(),
            OpenedAgentSession::Loaded(guard) => guard.commit(),
            OpenedAgentSession::Resumed(session) => session,
        }
    }
}

impl TaskProductApi {
    pub(super) fn open_agent_session(
        &self,
        task: &TaskRecord,
    ) -> Result<OpenedAgentSession<'_>, ProtocolError> {
        self.agent_registry
            .require(&task.agent_id)
            .map_err(super::super::protocol_error_from_runtime)?;
        let cancellation = TurnCancellation::new();
        match &task.agent_session_id {
            Some(session_id) => match self.agent_gateway.resume_session(AgentSessionResume {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                session_id: session_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                cancellation: cancellation.clone(),
            }) {
                Ok(session) => Ok(OpenedAgentSession::Resumed(session)),
                Err(error) if is_runtime_restart_resume_gap(&error) => self
                    .agent_gateway
                    .load_session(AgentSessionLoad {
                        agent_id: task.agent_id.clone(),
                        task_id: task.task_id.clone(),
                        session_id: session_id.clone(),
                        cwd: task.workspace_root.clone(),
                        model_id: task.model_id.clone(),
                        cancellation: cancellation.clone(),
                        secret_resolver: Some(self.task_secret_resolver(&task.task_id)),
                    })
                    .map(|loaded| {
                        OpenedAgentSession::Loaded(TaskSessionStartGuard::new(
                            &self.agent_gateway,
                            loaded.session,
                        ))
                    })
                    .or_else(|error| {
                        if is_restart_load_start_gap(&error) {
                            self.start_fresh_agent_session(task, cancellation)
                        } else {
                            Err(error)
                        }
                    })
                    .map_err(super::super::protocol_error_from_runtime),
                Err(error) => Err(super::super::protocol_error_from_runtime(error)),
            },
            None => self
                .start_fresh_agent_session(task, cancellation)
                .map_err(super::super::protocol_error_from_runtime),
        }
    }

    fn start_fresh_agent_session(
        &self,
        task: &TaskRecord,
        cancellation: TurnCancellation,
    ) -> Result<OpenedAgentSession<'_>, RuntimeError> {
        self.agent_gateway
            .start_session(AgentSessionStart {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                config_options: config_options_payload(task),
                context: Vec::new(),
                cancellation,
                secret_resolver: Some(self.task_secret_resolver(&task.task_id)),
            })
            .map(|session| {
                OpenedAgentSession::Started(TaskSessionStartGuard::new(
                    &self.agent_gateway,
                    session,
                ))
            })
    }
}

fn is_runtime_restart_resume_gap(error: &RuntimeError) -> bool {
    matches!(
        error,
        RuntimeError::CapabilityMissing(capability)
            if capability == "acp_session_resume_after_runtime_restart"
    )
}

fn is_restart_load_start_gap(error: &RuntimeError) -> bool {
    matches!(error, RuntimeError::NotReady(message) if message == "ACP session start timed out")
}

fn config_options_payload(task: &TaskRecord) -> Option<serde_json::Value> {
    serde_json::to_value(&task.config_options)
        .ok()
        .filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty))
}
