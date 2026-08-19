use openaide_app_server_protocol::agent::{AgentListSessionsParams, AgentListSessionsResult};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, MessageId, TurnId};
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::support::{
    SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult,
};
use openaide_app_server_protocol::task::{
    NativeSessionArchiveParams, NativeSessionForkParams, NativeSessionRestoreParams,
    TaskAcquireParams, TaskAdoptNativeSessionParams, TaskArchiveParams, TaskCancelParams,
    TaskClosePlanParams, TaskLifecycleChanged, TaskQueueAppendParams, TaskQueueMoveParams,
    TaskQueueRemoveParams, TaskQueueTakeParams, TaskQueueTakeResult, TaskReleaseParams,
    TaskRestoreParams, TaskSearchFilesParams, TaskSearchFilesResult, TaskSendParams,
    TaskSetConfigOptionParams, TaskSetPermissionPolicyParams, TaskSetPinnedParams,
    TaskSetTitleParams,
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

pub(crate) trait TaskStorageMaintenanceWorkflow: Send + Sync {
    /// Coalesces a best-effort local compaction and retention pass.
    fn request_task_storage_maintenance(&self);
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

    fn queue_append_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: TaskQueueAppendParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        Err(queue_unavailable())
    }

    fn queue_remove_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: TaskQueueRemoveParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        Err(queue_unavailable())
    }

    fn queue_take_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: TaskQueueTakeParams,
    ) -> Result<TaskQueueTakeResult, ProtocolError> {
        Err(queue_unavailable())
    }

    fn queue_move_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: TaskQueueMoveParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        Err(queue_unavailable())
    }
}

fn queue_unavailable() -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::CapabilityUnavailable,
        message: "Task Message Queue is unavailable".to_string(),
        recoverable: false,
        target: None,
    }
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
    fn set_permission_policy_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSetPermissionPolicyParams,
    ) -> Result<TaskSnapshot, ProtocolError>;

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

pub(crate) trait TaskPlanWorkflow: Send + Sync {
    fn close_plan_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskClosePlanParams,
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

    fn fork_native_session_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: NativeSessionForkParams,
    ) -> Result<NativeSessionForkMutation, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "Native Session fork is unavailable".to_string(),
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

#[derive(Debug, Clone)]
pub(crate) struct NativeSessionForkMutation {
    pub(crate) reference: openaide_app_server_protocol::snapshot::NativeSessionReference,
    pub(crate) project_id: openaide_app_server_protocol::ids::ProjectId,
    pub(crate) close_warning: bool,
}
