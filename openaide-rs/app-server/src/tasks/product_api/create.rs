use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::TaskCreateParams;
use uuid::Uuid;

use crate::projects::{resolve_project_context, ProjectTaskContext};
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::snapshots::task_snapshot::project_stored_task_snapshot;
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::tasks::snapshot::build_snapshot;
use crate::time::now_string;

use super::{
    protocol_error_from_runtime, response_snapshot_options, storage_error, TaskProductApi,
};

impl TaskProductApi {
    pub(super) fn create_task(
        &self,
        params: TaskCreateParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let project = self.resolve_create_project_context(&params)?;
        self.agent_registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        if let Some(task) = self.reusable_draft(&params, &project)? {
            return self.reopen_draft(task);
        }
        let now = now_string();
        let record = TaskRecord {
            task_id: format!("task_{}", Uuid::new_v4()),
            title: "New task".to_string(),
            agent_title: None,
            status: LegacyTaskStatus::Inactive,
            task_version: 1,
            message_history_version: 0,
            unread: false,
            created_at: now.clone(),
            updated_at: now.clone(),
            last_activity: now,
            agent_name: self
                .agent_registry
                .display_name(params.agent_id.as_str(), None)
                .map_err(protocol_error_from_runtime)?,
            agent_id: params.agent_id.into_string(),
            isolation: project.isolation,
            workspace_root: project.workspace_root,
            first_prompt_sent: false,
            agent_session_id: None,
            active_turn_id: None,
            archived: false,
            tombstoned: false,
            revision: 0,
            config_options: params
                .config_options
                .iter()
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect(),
            config_options_catalog: None,
            agent_commands_catalog: None,
            model_id: None,
            preparation: TaskPreparationRecord::Preparing,
        };
        let result = self
            .mutations
            .create_task(record.clone(), Vec::new(), response_snapshot_options())
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(super::prepare::missing_prepared_task_snapshot)?;
        let snapshot = project_stored_task_snapshot(snapshot)?;
        self.spawn_task_preparation(record);
        Ok(snapshot)
    }

    fn reusable_draft(
        &self,
        params: &TaskCreateParams,
        project: &ProjectTaskContext,
    ) -> Result<Option<TaskRecord>, ProtocolError> {
        self.store
            .list_tasks()
            .map_err(protocol_error_from_runtime)
            .map(|tasks| {
                tasks.into_iter().find(|task| {
                    !task.tombstoned
                        && !task.archived
                        && !task.first_prompt_sent
                        && task.active_turn_id.is_none()
                        && task.status == LegacyTaskStatus::Inactive
                        && task.agent_id == params.agent_id.as_str()
                        && task.workspace_root == project.workspace_root
                })
            })
    }

    fn reopen_draft(&self, task: TaskRecord) -> Result<TaskSnapshot, ProtocolError> {
        if matches!(task.preparation, TaskPreparationRecord::Preparing) {
            let snapshot =
                build_snapshot(&self.store, &task.task_id, 100).map_err(storage_error)?;
            return project_stored_task_snapshot(snapshot);
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
        project_stored_task_snapshot(snapshot)
    }

    fn resolve_create_project_context(
        &self,
        params: &TaskCreateParams,
    ) -> Result<ProjectTaskContext, ProtocolError> {
        resolve_project_context(
            self.project_resolver.as_ref(),
            &params.project_id,
            params.workspace_root.as_deref(),
        )
    }
}
