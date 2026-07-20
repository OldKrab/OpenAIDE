use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::agent::{AgentListSessionsParams, AgentListSessionsResult};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, MessageId, TurnId};
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::support::{
    SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult,
};
use openaide_app_server_protocol::task::{
    TaskAcquireParams, TaskAdoptNativeSessionParams, TaskCancelParams, TaskSearchFilesParams,
    TaskSearchFilesResult, TaskSendParams, TaskSetArchivedParams,
};
use openaide_app_server_protocol::task::{TaskReleaseParams, TaskSetConfigOptionParams};

use crate::agent::gateway::AgentGateway;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{AgentRuntime, AgentSessionKey};
use crate::attachment_runtime::AttachmentRuntime;
use crate::projects::ProjectResolver;
use crate::protocol::errors::RuntimeError;
use crate::protocol_edge::AppServerShutdownWorkflow;
#[cfg(test)]
use crate::protocol_edge::ShutdownBlockers;
use crate::server_requests::ServerRequestRuntime;
use crate::snapshots::task_snapshot::{
    project_stored_task_snapshot_with_history_sync, TaskHistorySyncSnapshotSource,
};
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
mod file_search;
mod list_sessions;
mod open;
mod prepare;
pub(crate) mod secret_resolver;
pub(crate) mod send;
mod session_cursor;
mod set_config_option;
mod support_recovery;

#[derive(Clone)]
pub(crate) struct TaskProductApi {
    store: Store,
    project_resolver: Arc<dyn ProjectResolver>,
    worktrees: crate::worktrees::WorktreeManager,
    agent_registry: AgentRegistryHandle,
    mutations: TaskMutations,
    agent_gateway: AgentGateway,
    attachments: AttachmentRuntime,
    workspace_files: crate::workspace_file_index::WorkspaceFileIndex,
    turn_runner: TurnRunner,
    native_sessions: crate::tasks::native_session_service::NativeSessionService,
    turn_acceptance: crate::tasks::turn_acceptance::TurnAcceptanceCoordinator,
    config_operations: crate::tasks::task_operation::TaskOperationCoordinator,
    // ACP may expose a newly started session before its Task metadata commit finishes.
    // Keep that session reserved so external-session listing never leaks a New Task.
    preparing_session_ids: Arc<Mutex<HashSet<AgentSessionKey>>>,
    history_sync: crate::tasks::history_sync::HistorySyncCoordinator,
    native_catalog_refresh: list_sessions::NativeCatalogRefreshCoordinator,
    #[allow(dead_code)]
    server_requests: ServerRequestRuntime,
    task_notifier: TaskUpdateNotifier,
}

pub(crate) trait TaskAcquireWorkflow: Send + Sync {
    fn acquire_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskAcquireParams,
    ) -> Result<TaskSnapshot, ProtocolError>;

    fn acquire_in_worktree_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskAcquireInWorktreeParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "Worktree Task acquisition is unavailable".to_string(),
            recoverable: false,
            target: None,
        })
    }
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

    /// Requests coalesced background reconciliation without blocking the caller.
    fn request_native_session_catalog_refresh(&self) {}
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

pub(crate) trait TaskFileSearchWorkflow: Send + Sync {
    fn search_files_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSearchFilesParams,
    ) -> Result<TaskSearchFilesResult, ProtocolError>;
}

pub(crate) trait TaskCancelWorkflow: Send + Sync {
    fn cancel_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskCancelParams,
    ) -> Result<TaskSnapshot, ProtocolError>;
    fn recover_stuck_sessions(
        &self,
        params: SupportRecoverStuckSessionsParams,
    ) -> Result<SupportRecoverStuckSessionsResult, ProtocolError>;
}

pub(crate) use open::TaskOpenWorkflow;

pub(crate) trait TaskSetConfigOptionWorkflow: Send + Sync {
    fn set_config_option_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetConfigOptionParams,
    ) -> Result<TaskSnapshot, ProtocolError>;
}

pub(crate) trait TaskReleaseWorkflow: Send + Sync {
    fn release_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskReleaseParams,
    ) -> Result<(), ProtocolError>;

    fn release_expired_client(
        &self,
        client_instance_id: &ClientInstanceId,
    ) -> Result<(), ProtocolError>;

    fn dispose_prepared_tasks_for_agent(&self, agent_id: &str) -> Result<(), ProtocolError>;
}

pub(crate) trait TaskArchiveWorkflow: Send + Sync {
    fn set_archived_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetArchivedParams,
    ) -> Result<(), ProtocolError>;
}

pub(crate) use attachments::AttachmentFileBrowserWorkflow;
pub(crate) use chat_page::TaskChatPageWorkflow;

impl TaskProductApi {
    /// Reads a Task at a client intent boundary and hides another client's New Task.
    pub(super) fn read_task_for_client(
        &self,
        task_id: &str,
        client_instance_id: &ClientInstanceId,
    ) -> Result<TaskRecord, ProtocolError> {
        let task = self.store.read_task(task_id).map_err(runtime_error)?;
        crate::tasks::access::require_client_task_access(&task, client_instance_id)
            .map_err(runtime_error)?;
        reject_tombstoned_task(&task)?;
        Ok(task)
    }

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
            notifier.clone(),
        );
        let agent_gateway = AgentGateway::new(agent_runtime.clone());
        let attachments = AttachmentRuntime::new();
        let agent_registry = agent_registry.into();
        let turn_runner = TurnRunner::new_with_server_requests(
            mutations.clone(),
            agent_runtime,
            server_requests.clone(),
        );
        let preparing_session_ids = Arc::new(Mutex::new(HashSet::new()));
        let native_sessions = crate::tasks::native_session_service::NativeSessionService::new(
            agent_registry.clone(),
            agent_gateway.clone(),
            mutations.clone(),
            turn_runner.clone(),
            server_requests.clone(),
            preparing_session_ids.clone(),
        );
        let api = Self {
            worktrees: crate::worktrees::WorktreeManager::new(store.clone()),
            store,
            project_resolver,
            agent_registry,
            mutations,
            agent_gateway,
            attachments,
            workspace_files: Default::default(),
            turn_runner,
            native_sessions,
            turn_acceptance: Default::default(),
            config_operations: Default::default(),
            preparing_session_ids,
            history_sync: crate::tasks::history_sync::HistorySyncCoordinator::default(),
            native_catalog_refresh: Default::default(),
            server_requests,
            task_notifier: notifier,
        };
        TaskTransitions::new(api.mutations.clone(), api.server_requests.clone())
            .recover_volatile_runtime_state()?;
        api.recover_abandoned_preparations()?;
        // A process epoch never inherits client ownership. Rebuild the bounded free pool
        // directly from durable zero-message Tasks before accepting protocol requests.
        api.mutations.reconcile_prepared_task_pool(true)?;
        Ok(api)
    }

    pub(super) fn publish_history_sync(
        &self,
        task_id: &str,
        state: openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot,
    ) {
        if !self.history_sync.set_current(task_id, state.clone()) {
            return;
        }
        if let Ok(task) = self.store.read_task(task_id) {
            self.task_notifier
                .history_sync_updated(task_id, task.revision, state);
        }
    }

    /// Projects durable Task data with the current process-local reconciliation state.
    pub(super) fn project_task_snapshot(
        &self,
        snapshot: crate::protocol::model::TaskSnapshot,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let history_sync = self
            .history_sync
            .history_sync_snapshot(&snapshot.task.task_id);
        project_stored_task_snapshot_with_history_sync(snapshot, history_sync)
    }

    pub(crate) fn history_sync_snapshots(&self) -> Arc<dyn TaskHistorySyncSnapshotSource> {
        Arc::new(self.history_sync.clone())
    }

    pub(crate) fn shutdown(&self) -> Result<(), RuntimeError> {
        self.turn_runner.shutdown()?;
        self.store.mark_clean_shutdown()
    }
}

impl crate::worktrees::WorktreeTaskCleanup for TaskProductApi {
    fn dispose_prepared_tasks_for_worktree(
        &self,
        worktree_id: &openaide_app_server_protocol::ids::WorktreeId,
    ) -> Result<(), RuntimeError> {
        let disposed = self
            .mutations
            .dispose_prepared_tasks_for_worktree(worktree_id.as_str())?;
        self.close_disposed_prepared_tasks(disposed);
        Ok(())
    }
}

impl AppServerShutdownWorkflow for TaskProductApi {
    fn shutdown(&self) -> Result<(), RuntimeError> {
        TaskProductApi::shutdown(self)
    }

    #[cfg(test)]
    fn shutdown_blockers(&self) -> Result<ShutdownBlockers, RuntimeError> {
        let mut owned_turns = self.turn_acceptance.owned_turns();
        owned_turns.extend(self.turn_runner.active_turns());
        Ok(ShutdownBlockers {
            active_turns: owned_turns.len(),
            pending_task_requests: self.server_requests.pending_count(),
        })
    }
}

impl TaskAcquireWorkflow for TaskProductApi {
    fn acquire_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskAcquireParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.create_task(client_instance_id, params)
    }

    fn acquire_in_worktree_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::task::TaskAcquireInWorktreeParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let worktree_id = params.worktree_id;
        self.create_task_in_workspace(
            client_instance_id,
            TaskAcquireParams {
                project_id: params.project_id,
                agent_id: params.agent_id,
                workspace_root: None,
            },
            Some(&worktree_id),
        )
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
    pub(crate) fn create_for_test(
        &self,
        params: TaskAcquireParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.create_task(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }

    pub(crate) fn open_for_test(
        &self,
        params: openaide_app_server_protocol::task::TaskOpenParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.open_task(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }

    pub(crate) fn mark_read_for_test(
        &self,
        params: openaide_app_server_protocol::task::TaskMarkReadParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.mark_task_read(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }

    pub(crate) fn set_config_option_for_test(
        &self,
        params: TaskSetConfigOptionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.set_config_option_on_task(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }

    pub(crate) fn release_for_test(&self, params: TaskReleaseParams) -> Result<(), ProtocolError> {
        self.release_task(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }

    pub(crate) fn cancel_for_test(
        &self,
        params: TaskCancelParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.cancel_task(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }

    pub(crate) fn set_archived_for_test(
        &self,
        params: TaskSetArchivedParams,
    ) -> Result<(), ProtocolError> {
        self.set_task_archived(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }

    pub(crate) fn send(&self, params: TaskSendParams) -> Result<TaskSendAccepted, ProtocolError> {
        self.send_message(
            &crate::attachment_runtime::AttachmentOwner::test_client_instance_id(),
            params,
        )
    }
}

impl TaskCancelWorkflow for TaskProductApi {
    fn cancel_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskCancelParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.cancel_task(client_instance_id, params)
    }

    fn recover_stuck_sessions(
        &self,
        params: SupportRecoverStuckSessionsParams,
    ) -> Result<SupportRecoverStuckSessionsResult, ProtocolError> {
        self.recover_stuck_sessions(params)
    }
}

impl TaskSetConfigOptionWorkflow for TaskProductApi {
    fn set_config_option_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetConfigOptionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.set_config_option_on_task(client_instance_id, params)
    }
}

impl TaskReleaseWorkflow for TaskProductApi {
    fn release_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskReleaseParams,
    ) -> Result<(), ProtocolError> {
        self.release_task(client_instance_id, params)
    }

    fn release_expired_client(
        &self,
        client_instance_id: &ClientInstanceId,
    ) -> Result<(), ProtocolError> {
        let leased = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?
            .into_iter()
            .find(|task| {
                matches!(
                    &task.lifecycle,
                    crate::storage::records::TaskLifecycle::New { lease: Some(lessee) }
                        if lessee == client_instance_id
                )
            });
        if let Some(task) = leased {
            self.release_task(
                client_instance_id,
                TaskReleaseParams {
                    task_id: task.task_id.into(),
                },
            )?;
        }
        Ok(())
    }

    fn dispose_prepared_tasks_for_agent(&self, agent_id: &str) -> Result<(), ProtocolError> {
        let disposed = self
            .mutations
            .dispose_prepared_tasks_for_agent(agent_id)
            .map_err(protocol_error_from_runtime)?;
        self.close_disposed_prepared_tasks(disposed);
        Ok(())
    }
}

impl TaskArchiveWorkflow for TaskProductApi {
    fn set_archived_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetArchivedParams,
    ) -> Result<(), ProtocolError> {
        self.set_task_archived(client_instance_id, params)
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
        RuntimeError::AuthRequired(message) => ProtocolError {
            code: ProtocolErrorCode::Unauthorized,
            message,
            recoverable: true,
            target: None,
        },
        RuntimeError::NodeJsRequired(message) => ProtocolError {
            code: ProtocolErrorCode::NodeJsRequired,
            message,
            recoverable: true,
            target: None,
        },
        RuntimeError::SetupRequired(message) => ProtocolError {
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
        RuntimeError::Conflict(message) => conflict_error(&message),
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
            current_task: None,
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
