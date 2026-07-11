use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::task::{TaskChatPageParams, TaskChatPageResult};

use super::{protocol_error_from_runtime, TaskProductApi};

pub(crate) trait TaskChatPageWorkflow: Send + Sync {
    fn chat_page(&self, params: TaskChatPageParams) -> Result<TaskChatPageResult, ProtocolError>;
}

impl TaskChatPageWorkflow for TaskProductApi {
    fn chat_page(&self, params: TaskChatPageParams) -> Result<TaskChatPageResult, ProtocolError> {
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
