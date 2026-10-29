//! Reads the Task state a subscription resume needs (ADR-0073) without cloning the
//! Task projection or reading Chat, which is what makes a resume cheaper than a baseline.

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};

use super::{effective_history_sync, subagent_overview, task_snapshot_error, TaskSnapshotStore};
use crate::protocol::errors::RuntimeError;
use crate::snapshots::TaskResumeBasis;

impl TaskSnapshotStore {
    pub(super) fn resume_basis(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<TaskResumeBasis, ProtocolError> {
        let (revision, workspace_root, reload_required) = self
            .store
            .task_journal()
            .inspect_task_record(task_id.as_str(), |task| {
                // A removed or another client's Prepared Task answers exactly like its baseline.
                if task.tombstoned {
                    return Err(RuntimeError::TaskNotFound(task.task_id.clone()));
                }
                crate::tasks::access::require_client_task_access(task, client_instance_id)?;
                Ok((
                    task.revision,
                    task.workspace_root.clone(),
                    task.native_session_reload_requirement.is_some(),
                ))
            })
            .and_then(|inspected| inspected)
            .map_err(task_snapshot_error)?;
        Ok(TaskResumeBasis {
            revision,
            history_sync: effective_history_sync(
                self.history_sync.history_sync_snapshot(task_id.as_str()),
                reload_required,
            ),
            subagents: subagent_overview(&self.store, task_id.as_str()),
            workspace_available: std::path::Path::new(&workspace_root).is_dir(),
        })
    }
}
