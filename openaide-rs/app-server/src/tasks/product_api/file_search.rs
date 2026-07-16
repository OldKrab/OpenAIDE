use std::path::Path;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::{
    TaskSearchFilesParams, TaskSearchFilesResult, WorkspaceFileSearchState,
};

use crate::workspace_file_index::WorkspaceFileIndexState;

use super::{TaskFileSearchWorkflow, TaskProductApi};

impl TaskFileSearchWorkflow for TaskProductApi {
    fn search_files_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSearchFilesParams,
    ) -> Result<TaskSearchFilesResult, ProtocolError> {
        let task = self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let search = self
            .workspace_files
            .search(Path::new(&task.workspace_root), &params.query);
        Ok(TaskSearchFilesResult {
            task_id: params.task_id,
            state: match search.state {
                WorkspaceFileIndexState::Ready => WorkspaceFileSearchState::Ready,
                WorkspaceFileIndexState::Refreshing => WorkspaceFileSearchState::Refreshing,
                WorkspaceFileIndexState::Unavailable => WorkspaceFileSearchState::Unavailable,
            },
            paths: search.paths,
            notice: search.notice,
        })
    }
}
