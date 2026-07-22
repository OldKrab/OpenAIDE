use uuid::Uuid;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::TaskAdoptNativeSessionParams;

use crate::agent::{AgentLoadedSession, AgentSessionLoad, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    IsolationKind, TaskSnapshot as StoredTaskSnapshot, TaskStatus as LegacyTaskStatus,
};
use crate::storage::records::{
    TaskLifecycle, TaskPreparationRecord, TaskRecord, TaskTitle, TaskTitleSource,
};
use crate::tasks::mutation::TaskCommitOptions;
use crate::tasks::mutation::TaskMutationResult;
use crate::tasks::task_start_transaction::TaskSessionStartGuard;
use crate::tasks::transitions::TaskTransitions;
use crate::time::now_string;

use super::{protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    pub(super) fn adopt_native_session_as_task(
        &self,
        params: TaskAdoptNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        crate::logging::info(
            "native_session_adoption_started",
            serde_json::json!({
                "agent_id": params.agent_id.as_str(),
                "session_id": params.native_session_id.as_str(),
            }),
        );
        let _adoption = self.native_adoption.lock().map_err(|_| {
            protocol_error_from_runtime(RuntimeError::Internal(
                "Native Session adoption lock poisoned".to_string(),
            ))
        })?;
        if let Some(existing) = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?
            .into_iter()
            .find(|task| {
                !task.tombstoned
                    && task.agent_id == params.agent_id.as_str()
                    && task.agent_session_id.as_deref() == Some(params.native_session_id.as_str())
            })
        {
            let result = self
                .mutations
                .commit_existing_task(
                    &existing.task_id,
                    super::response_snapshot_options(),
                    |_| Ok(TaskMutationResult::Unchanged),
                )
                .map_err(protocol_error_from_runtime)?;
            return self.project_task_snapshot(
                result.response_snapshot.ok_or_else(|| {
                    super::internal_error("missing existing adopted Task snapshot")
                })?,
            );
        }
        let reference = crate::native_sessions::catalog::NativeSessionRef::new(
            params.agent_id.as_str(),
            &params.native_session_id,
        );
        let catalog_entry = self.native_catalog.entry(&reference).ok_or_else(|| {
            crate::logging::info(
                "native_session_adoption_catalog_miss",
                serde_json::json!({
                    "agent_id": params.agent_id.as_str(),
                    "session_id": params.native_session_id.as_str(),
                }),
            );
            protocol_error_from_runtime(RuntimeError::TaskNotFound(
                "Native Session is no longer available".to_string(),
            ))
        })?;
        let project = self.project_resolver.resolve_task_context(
            &openaide_app_server_protocol::ids::ProjectId::from(catalog_entry.project_id.clone()),
        )?;
        self.agent_registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        self.mutations
            .ensure_native_session_unowned(params.agent_id.as_str(), &params.native_session_id)
            .map_err(protocol_error_from_runtime)?;

        let now = now_string();
        // Preserve the activity time shown in Native Session navigation when the row becomes a
        // Task. Adoption time is persistence metadata, not new conversation activity.
        let last_activity = self
            .native_catalog
            .entry(&reference)
            .and_then(|entry| entry.observation.last_activity)
            .unwrap_or_else(|| now.clone());
        let task_id = format!("task_{}", Uuid::new_v4());
        let loaded = match self.agent_gateway.load_session(AgentSessionLoad {
            agent_id: params.agent_id.as_str().to_string(),
            task_id: task_id.clone(),
            cwd: catalog_entry.workspace_root.clone(),
            model_id: None,
            session_id: params.native_session_id.clone(),
            cancellation: TurnCancellation::new(),
            secret_resolver: Some(self.task_secret_resolver(&task_id)),
        }) {
            Ok(loaded) => loaded,
            Err(error @ RuntimeError::TaskNotFound(_)) => {
                let reference = crate::native_sessions::catalog::NativeSessionRef::new(
                    params.agent_id.as_str(),
                    &params.native_session_id,
                );
                if self.native_catalog.remove(&reference).unwrap_or(false) {
                    self.task_notifier.navigation_changed();
                }
                return Err(protocol_error_from_runtime(error));
            }
            Err(error) => return Err(protocol_error_from_runtime(error)),
        };
        let mut session_start =
            TaskSessionStartGuard::new(&self.agent_gateway, loaded.session.clone());
        let title = catalog_entry
            .observation
            .title
            .clone()
            .and_then(|title| TaskTitle::new(title, TaskTitleSource::Agent));
        let session_id = session_start.session_id().to_string();

        let persist_result = self.persist_adopted_session_task(
            &params,
            &catalog_entry.workspace_root,
            &project.workspace_root,
            if catalog_entry.workspace_root == project.workspace_root {
                project.isolation
            } else {
                IsolationKind::GitWorktree
            },
            &task_id,
            &now,
            &last_activity,
            title,
            &session_id,
            loaded,
        );
        let snapshot = match persist_result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = session_start.close();
                let _ = self.fail_adopted_task_attach(&task_id, &session_id, &error);
                return Err(protocol_error_from_runtime(error));
            }
        };

        if let Err(error) = self
            .turn_runner
            .attach_session_events(task_id.clone(), &session_start.session().key())
        {
            let _ = session_start.close();
            if let Err(finalize_error) =
                self.fail_adopted_task_attach(&task_id, &session_id, &error)
            {
                return Err(protocol_error_from_runtime(RuntimeError::Internal(format!(
                    "{error}; failed to finalize adopted task after session event attachment failure: {finalize_error}"
                ))));
            }
            return Err(protocol_error_from_runtime(error));
        }
        let _session = session_start.commit();
        self.project_task_snapshot(snapshot)
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_adopted_session_task(
        &self,
        params: &TaskAdoptNativeSessionParams,
        workspace_root: &str,
        project_root: &str,
        isolation: IsolationKind,
        task_id: &str,
        now: &str,
        last_activity: &str,
        title: Option<TaskTitle>,
        session_id: &str,
        loaded: AgentLoadedSession,
    ) -> Result<StoredTaskSnapshot, RuntimeError> {
        let selected_agent_id = params.agent_id.as_str().to_string();
        let workspace_root = workspace_root.to_string();
        let session_id = session_id.to_string();
        let record = TaskRecord {
            task_id: task_id.to_string(),
            title,
            status: LegacyTaskStatus::Inactive,
            task_version: 1,
            message_history_version: 0,
            unread: false,
            attention: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
            last_activity: last_activity.to_string(),
            agent_name: self
                .agent_registry
                .display_name(params.agent_id.as_str(), None)?,
            agent_id: selected_agent_id.clone(),
            isolation,
            workspace_root: workspace_root.clone(),
            project_root: Some(project_root.to_string()),
            worktree_id: None,
            lifecycle: TaskLifecycle::Visible,
            agent_session_id: Some(session_id.clone()),
            active_turn_id: None,
            active_turn_started_at: None,
            archived: false,
            tombstoned: false,
            revision: 0,
            config_options_catalog: loaded.session.config_catalog.clone(),
            config_mutation: Default::default(),
            agent_commands_catalog: loaded.session.commands_catalog.clone(),
            model_id: loaded.session.model_id.clone(),
            supports_image_input: loaded.session.prompt_capabilities.image,
            preparation: TaskPreparationRecord::Ready,
        };
        let result = self.mutations.create_task_with_validation(
            record,
            loaded.replayed_messages,
            TaskCommitOptions {
                refresh_message_history: false,
                response_snapshot_tail_limit: Some(100),
            },
            |validation| validation.ensure_native_session_unowned(&selected_agent_id, &session_id),
        )?;
        result
            .response_snapshot
            .ok_or_else(|| RuntimeError::Internal("missing adopted task snapshot".to_string()))
    }

    fn fail_adopted_task_attach(
        &self,
        task_id: &str,
        session_id: &str,
        error: &RuntimeError,
    ) -> Result<(), RuntimeError> {
        TaskTransitions::new(self.mutations.clone(), self.server_requests.clone())
            .fail_adopted_task_attach(task_id, session_id, error)
    }
}
