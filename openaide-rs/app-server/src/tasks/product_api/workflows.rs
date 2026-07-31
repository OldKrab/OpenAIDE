use openaide_app_server_protocol::agent::{AgentListSessionsParams, AgentListSessionsResult};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, MessageId, TurnId};
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::support::{
    SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult,
};
use openaide_app_server_protocol::task::{
    NativeSessionArchiveParams, NativeSessionRestoreParams, TaskAcquireParams,
    TaskAdoptNativeSessionParams, TaskArchiveParams, TaskCancelParams, TaskLifecycleChanged,
    TaskReleaseParams, TaskRestoreParams, TaskSearchFilesParams, TaskSearchFilesResult,
    TaskSendParams, TaskSetConfigOptionParams, TaskSetPinnedParams, TaskSetTitleParams,
};

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

    fn request_native_session_catalog_load_more(
        &self,
        _project_id: &str,
        _target_row_count: usize,
    ) {
        self.request_native_session_catalog_refresh();
    }
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

pub(crate) trait TaskSetConfigOptionWorkflow: Send + Sync {
    fn set_config_option_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetConfigOptionParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskAgentConfigSnapshot, ProtocolError>;
}

pub(crate) trait TaskMetadataWorkflow: Send + Sync {
    fn set_title_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetTitleParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSummary, ProtocolError>;

    fn set_pinned_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetPinnedParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSummary, ProtocolError>;
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
    fn archive_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskArchiveParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError>;

    fn restore_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskRestoreParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError>;

    fn archive_native_session(
        &self,
        _params: NativeSessionArchiveParams,
    ) -> Result<NativeSessionArchiveMutation, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "Native Session archive is unavailable".to_string(),
            recoverable: false,
            target: None,
        })
    }

    fn restore_native_session(
        &self,
        _params: NativeSessionRestoreParams,
    ) -> Result<NativeSessionArchiveMutation, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "Native Session archive is unavailable".to_string(),
            recoverable: false,
            target: None,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NativeSessionArchiveMutation {
    pub(crate) reference: openaide_app_server_protocol::snapshot::NativeSessionReference,
    pub(crate) project_id: openaide_app_server_protocol::ids::ProjectId,
    pub(crate) archived: bool,
}
