use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::TaskAcquireParams;
use uuid::Uuid;

use crate::projects::{resolve_project_context, ProjectTaskContext};
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::storage::records::{TaskLifecycle, TaskPreparationRecord, TaskRecord};
use crate::tasks::mutation::TaskCommitOutcome;
use crate::tasks::snapshot::build_snapshot;
use crate::time::now_string;

use super::{
    protocol_error_from_runtime, response_snapshot_options, storage_error, TaskProductApi,
};

impl TaskProductApi {
    pub(super) fn create_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskAcquireParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.create_task_in_workspace(client_instance_id, params, None)
    }

    pub(super) fn create_task_in_workspace(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskAcquireParams,
        worktree_id: Option<&openaide_app_server_protocol::ids::WorktreeId>,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let project = self.resolve_create_project_context(&params)?;
        let workspace = self
            .worktrees
            .resolve_task_workspace(std::path::Path::new(&project.workspace_root), worktree_id)
            .map_err(protocol_error_from_runtime)?;
        self.agent_registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        let now = now_string();
        let record = TaskRecord {
            task_id: format!("task_{}", Uuid::new_v4()),
            title: Default::default(),
            status: LegacyTaskStatus::Inactive,
            task_version: 1,
            message_history_version: 0,
            unread: false,
            attention: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            last_activity: now,
            agent_name: self
                .agent_registry
                .display_name(params.agent_id.as_str(), None)
                .map_err(protocol_error_from_runtime)?,
            agent_id: params.agent_id.into_string(),
            isolation: workspace.isolation,
            workspace_root: workspace.path.to_string_lossy().to_string(),
            project_root: Some(project.workspace_root),
            worktree_id: workspace.worktree_id.map(|id| id.into_string()),
            lifecycle: TaskLifecycle::Prepared {
                lease: Some(client_instance_id.clone()),
            },
            agent_session_id: None,
            active_turn_id: None,
            active_turn_started_at: None,
            tombstoned: false,
            revision: 0,
            config_options_catalog: None,
            config_mutation: Default::default(),
            agent_commands_catalog: None,
            model_id: None,
            supports_image_input: false,
            preparation: TaskPreparationRecord::Preparing,
        };
        let result = self
            .mutations
            .acquire_prepared_task(record.clone(), Vec::new(), response_snapshot_options())
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(super::prepare::missing_prepared_task_snapshot)?;
        if let TaskCommitOutcome::Committed(facts) = &result.outcome {
            if facts.task_id == record.task_id {
                let snapshot = self.project_task_snapshot(snapshot)?;
                self.spawn_task_preparation(record);
                return Ok(snapshot);
            }
            let reused = self
                .store
                .read_task(&facts.task_id)
                .map_err(protocol_error_from_runtime)?;
            if self.native_sessions.is_live(&facts.task_id) {
                return self.project_task_snapshot(snapshot);
            }
            return self.reopen_new_task(reused);
        }
        let existing = self
            .store
            .read_task(&snapshot.task.task_id)
            .map_err(protocol_error_from_runtime)?;
        self.reopen_new_task(existing)
    }

    fn reopen_new_task(&self, task: TaskRecord) -> Result<TaskSnapshot, ProtocolError> {
        if matches!(task.preparation, TaskPreparationRecord::Preparing) {
            let snapshot =
                build_snapshot(&self.store, &task.task_id, 100).map_err(storage_error)?;
            return self.project_task_snapshot(snapshot);
        }

        let task_id = task.task_id.clone();
        let result = self
            .mutations
            .commit_existing_task(&task_id, response_snapshot_options(), |ctx| {
                if matches!(ctx.task().preparation, TaskPreparationRecord::Preparing) {
                    return Ok(crate::tasks::mutation::TaskMutationResult::Unchanged);
                }
                ctx.task_mut().preparation = TaskPreparationRecord::Preparing;
                Ok(crate::tasks::mutation::TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(super::prepare::missing_prepared_task_snapshot)?;
        let prepared = self
            .store
            .read_task(&task_id)
            .map_err(protocol_error_from_runtime)?;
        if matches!(
            result.outcome,
            crate::tasks::mutation::TaskCommitOutcome::Committed(_)
        ) {
            self.spawn_task_preparation(prepared);
        }
        self.project_task_snapshot(snapshot)
    }

    fn resolve_create_project_context(
        &self,
        params: &TaskAcquireParams,
    ) -> Result<ProjectTaskContext, ProtocolError> {
        resolve_project_context(
            self.project_resolver.as_ref(),
            &params.project_id,
            params.workspace_root.as_deref(),
        )
    }
}
