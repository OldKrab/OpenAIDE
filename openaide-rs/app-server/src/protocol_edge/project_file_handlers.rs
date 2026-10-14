use super::{responses, RpcGateway};
use crate::{client_lifecycle::ConnectionId, logging};
use openaide_app_server_protocol::{
    envelopes::RequestMeta,
    errors::ProtocolError,
    errors::ProtocolErrorCode,
    ids::{ClientInstanceId, ProjectId, TaskId, WorktreeId},
    project_files::{ProjectChangeBase, ProjectFilesParams, ProjectFilesResult},
};
use serde_json::Value;
use std::{path::PathBuf, time::Instant};

pub(super) struct PreparedProjectFiles {
    pub owner: ClientInstanceId,
    root: PathBuf,
    params: ProjectFilesParams,
    method: String,
    /// Which kind of root was authorized; logged instead of the path.
    workspace_kind: &'static str,
}
impl PreparedProjectFiles {
    pub fn run(&self, id: &str, meta: &RequestMeta) -> ProjectFilesResult {
        let started = Instant::now();
        logging::info(
            "project_files_started",
            serde_json::json!({"request_id":id,"client_request_id":meta.client_request_id,"client_id":self.owner,"operation":self.method,"attempt":1,"workspace_kind":self.workspace_kind,"base":self.base()}),
        );
        let result = crate::project_files::run(&self.root, &self.method, &self.params);
        logging::info(
            "project_files_completed",
            serde_json::json!({"request_id":id,"client_request_id":meta.client_request_id,"client_id":self.owner,"operation":self.method,"attempt":1,"workspace_kind":self.workspace_kind,"base":self.base(),"default_branch_found":result.default_branch.is_some(),"outcome":if result.error.is_some(){"failure"}else{"success"},"error_kind":result.error,"truncated":result.truncated,"count":result.entries.len(),"duration_ms":started.elapsed().as_millis()}),
        );
        result
    }
    fn base(&self) -> &'static str {
        match self.params.base {
            Some(ProjectChangeBase::DefaultBranch) => "defaultBranch",
            _ => "uncommitted",
        }
    }
}
impl RpcGateway {
    pub(super) fn prepare_project_files(
        &self,
        connection: &ConnectionId,
        method: &str,
        params: Value,
    ) -> Result<PreparedProjectFiles, ProtocolError> {
        let client = self
            .client_hub
            .context_for_connection(connection)
            .ok_or_else(|| responses::not_initialized(method.to_string()))?;
        let params: ProjectFilesParams =
            serde_json::from_value(params).map_err(responses::invalid_params)?;
        let (root, workspace_kind) = self.file_workspace_root(
            &client.client_instance_id,
            params.task_id.as_ref(),
            params.project_id.as_ref(),
            params.worktree_id.as_ref(),
        )?;
        Ok(PreparedProjectFiles {
            owner: client.client_instance_id,
            root: PathBuf::from(root),
            params,
            method: method.into(),
            workspace_kind,
        })
    }

    /// Resolves the root for read-only file access. A Task authorizes its Task Workspace and a
    /// Project its root; a worktree identifier may only select another recorded worktree of
    /// that root's repository. The browser never supplies a path.
    pub(super) fn file_workspace_root(
        &self,
        client: &ClientInstanceId,
        task_id: Option<&TaskId>,
        project_id: Option<&ProjectId>,
        worktree_id: Option<&WorktreeId>,
    ) -> Result<(String, &'static str), ProtocolError> {
        let (anchor, kind) = match (task_id, project_id) {
            (Some(task_id), None) => (
                self.task_snapshots
                    .workspace_root_for_client(client, task_id)?,
                "taskWorkspace",
            ),
            (None, Some(project_id)) => (
                self.snapshots
                    .project_collection_snapshot()?
                    .projects
                    .into_iter()
                    .find(|project| project.project_id == *project_id)
                    .ok_or_else(|| file_workspace_error("Selected Project is unavailable"))?
                    .workspace_root,
                "projectRoot",
            ),
            _ => {
                return Err(file_workspace_error(
                    "File access needs exactly one of a Task and a Project",
                ))
            }
        };
        let Some(worktree_id) = worktree_id else {
            return Ok((anchor, kind));
        };
        let root = self
            .worktrees
            .sibling_worktree_root(std::path::Path::new(&anchor), worktree_id)
            .map_err(|_| file_workspace_error("Selected worktree is unavailable"))?;
        Ok((root.to_string_lossy().into_owned(), "worktree"))
    }
}

fn file_workspace_error(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InvalidRequest,
        message: message.to_string(),
        recoverable: false,
        target: None,
    }
}
