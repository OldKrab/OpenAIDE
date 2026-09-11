use super::{
    NativeSessionArchiveMutation, NativeSessionForkMutation, NativeSessionMetadataMutation,
    TaskArchiveWorkflow, TaskProductApi,
};
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::{
    NativeSessionArchiveParams, NativeSessionDeleteParams, NativeSessionDeleteResult,
    NativeSessionForkParams, NativeSessionRestoreParams, NativeSessionSetPinnedParams,
    NativeSessionSetTitleParams, TaskArchiveOlderParams, TaskArchiveOlderResult, TaskArchiveParams,
    TaskLifecycleChanged, TaskRestoreParams,
};

impl TaskArchiveWorkflow for TaskProductApi {
    fn archive_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskArchiveParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        self.archive_task(client_instance_id, params)
    }

    fn restore_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskRestoreParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        self.restore_task(client_instance_id, params)
    }

    fn archive_older_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskArchiveOlderParams,
    ) -> Result<TaskArchiveOlderResult, ProtocolError> {
        self.archive_older_tasks(client_instance_id, params)
    }

    fn archive_native_session(
        &self,
        params: NativeSessionArchiveParams,
    ) -> Result<NativeSessionArchiveMutation, ProtocolError> {
        self.set_native_session_archived(params.agent_id.as_str(), &params.native_session_id, true)
    }

    fn restore_native_session(
        &self,
        params: NativeSessionRestoreParams,
    ) -> Result<NativeSessionArchiveMutation, ProtocolError> {
        self.set_native_session_archived(params.agent_id.as_str(), &params.native_session_id, false)
    }

    fn set_native_session_title(
        &self,
        params: NativeSessionSetTitleParams,
    ) -> Result<NativeSessionMetadataMutation, ProtocolError> {
        self.set_native_session_title(
            params.agent_id.as_str(),
            &params.native_session_id,
            params.title,
        )
    }

    fn set_native_session_pinned(
        &self,
        params: NativeSessionSetPinnedParams,
    ) -> Result<NativeSessionMetadataMutation, ProtocolError> {
        self.set_native_session_pinned(
            params.agent_id.as_str(),
            &params.native_session_id,
            params.pinned,
        )
    }

    fn fork_native_session_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: NativeSessionForkParams,
    ) -> Result<NativeSessionForkMutation, ProtocolError> {
        self.fork_native_session(client_instance_id, params)
    }

    fn delete_native_session_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: NativeSessionDeleteParams,
        operation_id: &str,
    ) -> Result<NativeSessionDeleteResult, ProtocolError> {
        self.delete_native_session(client_instance_id, params, operation_id)
    }
}
