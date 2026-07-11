use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::agent::{AgentListSessionsParams, AgentListSessionsResult};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, MessageId, TurnId};
use openaide_app_server_protocol::snapshot::{TaskNavigationSnapshot, TaskSnapshot};
use openaide_app_server_protocol::support::{
    SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult,
};
use openaide_app_server_protocol::task::{
    TaskAdoptNativeSessionParams, TaskCancelParams, TaskCreateParams, TaskSendParams,
    TaskSetArchivedParams,
};
use openaide_app_server_protocol::task::{TaskDiscardParams, TaskSetConfigOptionParams};

use crate::agent::gateway::AgentGateway;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::AgentRuntime;
use crate::attachment_runtime::AttachmentRuntime;
use crate::projects::ProjectResolver;
use crate::protocol::errors::RuntimeError;
use crate::protocol_edge::{AppServerShutdownWorkflow, ShutdownBlockers};
use crate::server_requests::ServerRequestRuntime;
use crate::storage::records::TaskRecord;
use crate::storage::Store;
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutations};
use crate::tasks::runtime_state::RuntimeState;
use crate::tasks::transitions::TaskTransitions;
use crate::tasks::turns::TurnRunner;

mod adopt_native_session;
mod archive;
mod attachments;
mod cancel;
mod chat_page;
mod create;
mod discard;
mod list_sessions;
mod open;
mod prepare;
mod secret_resolver;
mod send;
mod set_config_option;
mod support_recovery;
mod tool_detail;

#[derive(Clone)]
pub(crate) struct TaskProductApi {
    store: Store,
    project_resolver: Arc<dyn ProjectResolver>,
    agent_registry: AgentRegistryHandle,
    mutations: TaskMutations,
    agent_gateway: AgentGateway,
    attachments: AttachmentRuntime,
    turn_runner: TurnRunner,
    #[allow(dead_code)]
    server_requests: ServerRequestRuntime,
}

pub(crate) trait TaskCreateWorkflow: Send + Sync {
    fn create(&self, params: TaskCreateParams) -> Result<TaskSnapshot, ProtocolError>;
}

pub(crate) trait TaskAdoptNativeSessionWorkflow: Send + Sync {
    fn adopt_native_session(
        &self,
        params: TaskAdoptNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError>;
}

pub(crate) trait AgentListSessionsWorkflow: Send + Sync {
    fn list_agent_sessions(
        &self,
        params: AgentListSessionsParams,
    ) -> Result<AgentListSessionsResult, ProtocolError>;
}

#[derive(Debug)]
pub(crate) struct TaskSendAccepted {
    pub task: TaskSnapshot,
    pub turn_id: TurnId,
    pub user_message_id: MessageId,
}

pub(crate) trait TaskSendWorkflow: Send + Sync {
    fn send_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSendParams,
    ) -> Result<TaskSendAccepted, ProtocolError>;
}

pub(crate) trait TaskCancelWorkflow: Send + Sync {
    fn cancel(&self, params: TaskCancelParams) -> Result<TaskSnapshot, ProtocolError>;
    fn recover_stuck_sessions(
        &self,
        params: SupportRecoverStuckSessionsParams,
    ) -> Result<SupportRecoverStuckSessionsResult, ProtocolError>;
}

pub(crate) use open::TaskOpenWorkflow;

pub(crate) trait TaskSetConfigOptionWorkflow: Send + Sync {
    fn set_config_option(
        &self,
        params: TaskSetConfigOptionParams,
    ) -> Result<TaskSnapshot, ProtocolError>;
}

pub(crate) trait TaskDiscardWorkflow: Send + Sync {
    fn discard(&self, params: TaskDiscardParams) -> Result<TaskNavigationSnapshot, ProtocolError>;
}

pub(crate) trait TaskArchiveWorkflow: Send + Sync {
    fn set_archived(
        &self,
        params: TaskSetArchivedParams,
    ) -> Result<TaskNavigationSnapshot, ProtocolError>;
}

pub(crate) use attachments::AttachmentFileBrowserWorkflow;
pub(crate) use chat_page::TaskChatPageWorkflow;
pub(crate) use tool_detail::TaskToolDetailWorkflow;

impl TaskProductApi {
    #[cfg(test)]
    pub(crate) fn attachment_runtime(&self) -> AttachmentRuntime {
        self.attachments.clone()
    }

    #[cfg(test)]
    pub(crate) fn new(
        store: Store,
        project_resolver: Arc<dyn ProjectResolver>,
        agent_registry: impl Into<AgentRegistryHandle>,
        agent_runtime: Arc<dyn AgentRuntime>,
        notifier: TaskUpdateNotifier,
    ) -> Result<Self, RuntimeError> {
        Self::new_with_server_requests(
            store,
            project_resolver,
            agent_registry,
            agent_runtime,
            notifier,
            ServerRequestRuntime::new(),
        )
    }

    pub(crate) fn new_with_server_requests(
        store: Store,
        project_resolver: Arc<dyn ProjectResolver>,
        agent_registry: impl Into<AgentRegistryHandle>,
        agent_runtime: Arc<dyn AgentRuntime>,
        notifier: TaskUpdateNotifier,
        server_requests: ServerRequestRuntime,
    ) -> Result<Self, RuntimeError> {
        let initial_revision = store.max_task_revision()?;
        let mutations = TaskMutations::new(
            store.clone(),
            Arc::new(Mutex::new(())),
            Arc::new(Mutex::new(RuntimeState::with_revision(initial_revision))),
            notifier,
        );
        let agent_gateway = AgentGateway::new(agent_runtime.clone());
        let attachments = AttachmentRuntime::new();
        let turn_runner = TurnRunner::new_with_server_requests(
            mutations.clone(),
            agent_runtime,
            server_requests.clone(),
        );
        let api = Self {
            store,
            project_resolver,
            agent_registry: agent_registry.into(),
            mutations,
            agent_gateway,
            attachments,
            turn_runner,
            server_requests,
        };
        TaskTransitions::new(api.mutations.clone()).recover_volatile_runtime_state()?;
        api.recover_abandoned_preparations()?;
        Ok(api)
    }

    pub(crate) fn shutdown(&self) -> Result<(), RuntimeError> {
        self.turn_runner.shutdown()?;
        self.store.mark_clean_shutdown()
    }
}

impl AppServerShutdownWorkflow for TaskProductApi {
    fn shutdown(&self) -> Result<(), RuntimeError> {
        TaskProductApi::shutdown(self)
    }

    fn shutdown_blockers(&self) -> Result<ShutdownBlockers, RuntimeError> {
        Ok(ShutdownBlockers {
            active_turns: self.turn_runner.active_turn_count(),
            pending_task_requests: self.server_requests.pending_count(),
        })
    }
}

impl TaskCreateWorkflow for TaskProductApi {
    fn create(&self, params: TaskCreateParams) -> Result<TaskSnapshot, ProtocolError> {
        self.create_task(params)
    }
}

impl TaskAdoptNativeSessionWorkflow for TaskProductApi {
    fn adopt_native_session(
        &self,
        params: TaskAdoptNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.adopt_native_session_as_task(params)
    }
}

impl TaskSendWorkflow for TaskProductApi {
    fn send_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSendParams,
    ) -> Result<TaskSendAccepted, ProtocolError> {
        self.send_message(client_instance_id, params)
    }
}

#[cfg(test)]
impl TaskProductApi {
    pub(crate) fn send(&self, params: TaskSendParams) -> Result<TaskSendAccepted, ProtocolError> {
        self.send_message(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }
}

impl TaskCancelWorkflow for TaskProductApi {
    fn cancel(&self, params: TaskCancelParams) -> Result<TaskSnapshot, ProtocolError> {
        self.cancel_task(params)
    }

    fn recover_stuck_sessions(
        &self,
        params: SupportRecoverStuckSessionsParams,
    ) -> Result<SupportRecoverStuckSessionsResult, ProtocolError> {
        self.recover_stuck_sessions(params)
    }
}

impl TaskSetConfigOptionWorkflow for TaskProductApi {
    fn set_config_option(
        &self,
        params: TaskSetConfigOptionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.set_config_option_on_task(params)
    }
}

impl TaskDiscardWorkflow for TaskProductApi {
    fn discard(&self, params: TaskDiscardParams) -> Result<TaskNavigationSnapshot, ProtocolError> {
        self.discard_task(params)
    }
}

impl TaskArchiveWorkflow for TaskProductApi {
    fn set_archived(
        &self,
        params: TaskSetArchivedParams,
    ) -> Result<TaskNavigationSnapshot, ProtocolError> {
        self.set_task_archived(params)
    }
}

pub(super) fn protocol_error_from_runtime(error: RuntimeError) -> ProtocolError {
    match error {
        RuntimeError::CapabilityMissing(message) => ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message,
            recoverable: true,
            target: None,
        },
        RuntimeError::InvalidParams(field) => ProtocolError {
            code: ProtocolErrorCode::ValidationFailed,
            message: format!("Invalid field: {field}"),
            recoverable: false,
            target: None,
        },
        other => ProtocolError {
            code: ProtocolErrorCode::Internal,
            message: other.to_string(),
            recoverable: true,
            target: None,
        },
    }
}

pub(super) fn runtime_error(error: RuntimeError) -> ProtocolError {
    match error {
        RuntimeError::TaskNotFound(message) => ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message,
            recoverable: false,
            target: None,
        },
        RuntimeError::InvalidParams(field) => validation_error(&field, "Invalid field"),
        other => storage_error(other),
    }
}

pub(super) fn reject_tombstoned_task(task: &TaskRecord) -> Result<(), ProtocolError> {
    if task.tombstoned {
        return Err(ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message: format!("task not found: {}", task.task_id),
            recoverable: false,
            target: None,
        });
    }
    Ok(())
}

pub(super) fn validation_error(field: &str, message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::ValidationFailed,
        message: message.to_string(),
        recoverable: false,
        target: Some(openaide_app_server_protocol::errors::ErrorTarget {
            method: None,
            field: Some(field.to_string()),
        }),
    }
}

pub(super) fn conflict_error(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Conflict,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}

pub(super) fn internal_error(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Internal,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}

pub(super) fn storage_error(error: impl std::fmt::Display) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Internal,
        message: error.to_string(),
        recoverable: true,
        target: None,
    }
}

fn response_snapshot_options() -> TaskCommitOptions {
    TaskCommitOptions {
        refresh_message_history: true,
        response_snapshot_tail_limit: Some(100),
    }
}

#[cfg(test)]
mod tests;
