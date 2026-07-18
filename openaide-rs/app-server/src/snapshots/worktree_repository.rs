use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::WorktreeRepositoryId;
use openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot;

use crate::worktrees::WorktreeManager;

pub trait WorktreeRepositorySnapshotSource: Send + Sync {
    fn snapshot(
        &self,
        repository_id: &WorktreeRepositoryId,
    ) -> Result<WorktreeRepositorySnapshot, ProtocolError>;
}

impl WorktreeRepositorySnapshotSource for WorktreeManager {
    fn snapshot(
        &self,
        repository_id: &WorktreeRepositoryId,
    ) -> Result<WorktreeRepositorySnapshot, ProtocolError> {
        WorktreeManager::snapshot(self, repository_id).map_err(|error| ProtocolError {
            code: if matches!(
                error,
                crate::protocol::errors::RuntimeError::TaskNotFound(_)
            ) {
                ProtocolErrorCode::NotFound
            } else {
                ProtocolErrorCode::Internal
            },
            message: format!("Failed to read Worktree Repository: {error}"),
            recoverable: true,
            target: None,
        })
    }
}
