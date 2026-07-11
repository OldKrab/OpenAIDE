use uuid::Uuid;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::TaskAdoptNativeSessionParams;

use crate::agent::{AgentLoadedSession, AgentSessionLoad, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    IsolationKind, NormalizedMessage, TaskSnapshot as StoredTaskSnapshot,
    TaskStatus as LegacyTaskStatus,
};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
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
            .ensure_native_session_unowned(
                params.agent_id.as_str(),
                &project.workspace_root,
                &params.native_session_id,
            )
            .map_err(protocol_error_from_runtime)?;

        let now = now_string();
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
        let agent_title = params
            .title
            .clone()
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty());
        let fallback_title = title_from_loaded_messages(&loaded.replayed_messages);
        let session_id = session_start.session_id().to_string();

        let persist_result = self.persist_adopted_session_task(
            &params,
            &project.workspace_root,
            project.isolation,
            &task_id,
            &now,
            &fallback_title,
            agent_title,
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
            .attach_session_events(task_id.clone(), &session_id)
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
        crate::snapshots::task_snapshot::project_stored_task_snapshot(snapshot)
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_adopted_session_task(
        &self,
        params: &TaskAdoptNativeSessionParams,
        workspace_root: &str,
        isolation: IsolationKind,
        task_id: &str,
        now: &str,
        fallback_title: &str,
        agent_title: Option<String>,
        session_id: &str,
        loaded: AgentLoadedSession,
    ) -> Result<StoredTaskSnapshot, RuntimeError> {
        let selected_agent_id = params.agent_id.as_str().to_string();
        let workspace_root = workspace_root.to_string();
        let session_id = session_id.to_string();
        let record = TaskRecord {
            task_id: task_id.to_string(),
            title: fallback_title.to_string(),
            agent_title,
            status: LegacyTaskStatus::Inactive,
            task_version: 1,
            message_history_version: 0,
            unread: false,
            created_at: now.to_string(),
            updated_at: now.to_string(),
            last_activity: now.to_string(),
            agent_name: self
                .agent_registry
                .display_name(params.agent_id.as_str(), None)?,
            agent_id: selected_agent_id.clone(),
            isolation,
            workspace_root: workspace_root.clone(),
            first_prompt_sent: true,
            agent_session_id: Some(session_id.clone()),
            active_turn_id: None,
            archived: false,
            tombstoned: false,
            revision: 0,
            config_options: loaded.session.config_options.clone(),
            config_options_catalog: loaded.session.config_catalog.clone(),
            agent_commands_catalog: loaded.session.commands_catalog.clone(),
            model_id: loaded.session.model_id.clone(),
            preparation: TaskPreparationRecord::Ready,
        };
        let result = self.mutations.create_task_with_validation(
            record,
            loaded.replayed_messages,
            TaskCommitOptions {
                refresh_message_history: false,
                response_snapshot_tail_limit: Some(100),
            },
            |validation| {
                validation.ensure_native_session_unowned(
                    &selected_agent_id,
                    &workspace_root,
                    &session_id,
                )
            },
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
        TaskTransitions::new(self.mutations.clone())
            .fail_adopted_task_attach(task_id, session_id, error)
    }
}

fn title_from_loaded_messages(messages: &[NormalizedMessage]) -> String {
    messages
        .iter()
        .rev()
        .find_map(message_text)
        .and_then(|text| text.trim().lines().next().map(str::to_string))
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "Imported session".to_string())
}

fn message_text(message: &NormalizedMessage) -> Option<&str> {
    match message {
        NormalizedMessage::User { text, .. }
        | NormalizedMessage::AgentText { text, .. }
        | NormalizedMessage::Thought { text, .. } => Some(text),
        NormalizedMessage::Activity { .. }
        | NormalizedMessage::Permission { .. }
        | NormalizedMessage::Question { .. }
        | NormalizedMessage::Interruption { .. } => None,
    }
}
