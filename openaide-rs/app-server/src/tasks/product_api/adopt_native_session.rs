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
use crate::tasks::task_start_transaction::TaskSessionStartGuard;
use crate::tasks::transitions::TaskTransitions;
use crate::time::now_string;

use super::{protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    pub(super) fn adopt_native_session_as_task(
        &self,
        params: TaskAdoptNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let project = self
            .project_resolver
            .resolve_task_context(&params.project_id)?;
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
            .history_sync
            .cached_session(
                params.agent_id.as_str(),
                &project.workspace_root,
                &params.native_session_id,
            )
            .and_then(|session| session.last_activity.or(session.updated_at))
            .unwrap_or_else(|| now.clone());
        let task_id = format!("task_{}", Uuid::new_v4());
        let loaded = self
            .agent_gateway
            .load_session(AgentSessionLoad {
                agent_id: params.agent_id.as_str().to_string(),
                task_id: task_id.clone(),
                cwd: project.workspace_root.clone(),
                model_id: None,
                session_id: params.native_session_id.clone(),
                cancellation: TurnCancellation::new(),
                secret_resolver: Some(self.task_secret_resolver(&task_id)),
            })
            .map_err(protocol_error_from_runtime)?;
        let mut session_start =
            TaskSessionStartGuard::new(&self.agent_gateway, loaded.session.clone());
        let title = params
            .title
            .clone()
            .and_then(|title| TaskTitle::new(title, TaskTitleSource::Agent));
        let session_id = session_start.session_id().to_string();

        let persist_result = self.persist_adopted_session_task(
            &params,
            &project.workspace_root,
            project.isolation,
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
            lifecycle: TaskLifecycle::Visible,
            agent_session_id: Some(session_id.clone()),
            active_turn_id: None,
            active_turn_started_at: None,
            archived: false,
            tombstoned: false,
            revision: 0,
            config_options: loaded.session.config_options.clone(),
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
