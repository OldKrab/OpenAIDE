use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::{
    ComposerHistoryParams, ComposerHistoryResult, TaskChatPageParams, TaskChatPageResult,
};

use super::{protocol_error_from_runtime, TaskProductApi};

/// Authorized, read-only Task history projections exposed through the protocol edge.
pub(crate) trait TaskHistoryWorkflow: Send + Sync {
    fn chat_page_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskChatPageParams,
    ) -> Result<TaskChatPageResult, ProtocolError>;

    fn composer_history_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: ComposerHistoryParams,
    ) -> Result<ComposerHistoryResult, ProtocolError>;
}

impl TaskHistoryWorkflow for TaskProductApi {
    fn composer_history_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: ComposerHistoryParams,
    ) -> Result<ComposerHistoryResult, ProtocolError> {
        super::composer_history::query(self, client_instance_id, params)
    }

    fn chat_page_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskChatPageParams,
    ) -> Result<TaskChatPageResult, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let page = self
            .store
            .page_before(
                params.task_id.as_str(),
                params.before_cursor.as_str(),
                params.limit as usize,
            )
            .map_err(protocol_error_from_runtime)?;
        let revision = self
            .store
            .max_task_revision()
            .map_err(protocol_error_from_runtime)?;
        Ok(TaskChatPageResult {
            task_id: params.task_id,
            items: page
                .items
                .iter()
                .map(crate::snapshots::task_snapshot::project_chat_item)
                .collect(),
            has_before: page.has_before,
            total_count: page.total_count,
            revision,
            start_cursor: page.start_cursor.map(Into::into),
            end_cursor: page.end_cursor.map(Into::into),
        })
    }
}
