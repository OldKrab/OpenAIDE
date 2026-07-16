use uuid::Uuid;

use crate::agent::{AgentSessionLoad, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{TaskSnapshot, TaskStatus};
use crate::protocol::params::TaskCreateParams;
use crate::storage::records::{
    TaskLifecycle, TaskPreparationRecord, TaskRecord, TaskTitle, TaskTitleSource,
};
use crate::tasks::config_options::selected_config_options;
use crate::tasks::task_start_transaction::TaskSessionStartGuard;
use crate::time::now_string;

use super::helpers::required_optional_text;
use super::{create_snapshot_commit_options, TaskTurnLifecycle};

impl TaskTurnLifecycle {
    pub(in crate::tasks::turn_lifecycle) fn create_adopted_session(
        &self,
        params: TaskCreateParams,
    ) -> Result<TaskSnapshot, RuntimeError> {
        self.agent_registry.validate_task_create(&params)?;
        let external_session_id =
            required_optional_text(params.external_session_id.clone(), "external_session_id")?;
        let selected_config_options = selected_config_options(params.config_options.as_ref())?;
        if !selected_config_options.is_empty() {
            return Err(RuntimeError::InvalidParams("config_options".to_string()));
        }

        self.mutations
            .ensure_native_session_unowned(&params.selected_agent_id, &external_session_id)?;

        let now = now_string();
        let task_id = format!("task_{}", Uuid::new_v4());
        let loaded = self.agent_gateway.load_session(AgentSessionLoad {
            agent_id: params.selected_agent_id.clone(),
            task_id: task_id.clone(),
            cwd: params.workspace_root.clone(),
            model_id: params.model_id.clone(),
            session_id: external_session_id,
            cancellation: TurnCancellation::new(),
            secret_resolver: None,
        })?;
        let mut session_start = TaskSessionStartGuard::new(&self.agent_gateway, loaded.session);
        let title = TaskTitle::new(params.title, TaskTitleSource::Agent);

        let persist_result: Result<TaskSnapshot, RuntimeError> = (|| {
            let session = session_start.session();
            let selected_agent_id = params.selected_agent_id.clone();
            let session_id = session.session_id.clone();
            let record = TaskRecord {
                task_id: task_id.clone(),
                title,
                status: TaskStatus::Inactive,
                task_version: 1,
                message_history_version: 0,
                unread: false,
                attention: None,
                created_at: now.clone(),
                updated_at: now.clone(),
                last_activity: now.clone(),
                agent_name: self.agent_registry.display_name(
                    &params.selected_agent_id,
                    params.selected_agent_label.as_deref(),
                )?,
                agent_id: params.selected_agent_id,
                isolation: params.selected_isolation,
                workspace_root: params.workspace_root,
                lifecycle: TaskLifecycle::Visible,
                agent_session_id: Some(session.session_id.clone()),
                active_turn_id: None,
                active_turn_started_at: None,
                archived: false,
                tombstoned: false,
                revision: 0,
                config_options: session.config_options.clone(),
                config_options_catalog: session.config_catalog.clone(),
                config_mutation: Default::default(),
                agent_commands_catalog: session.commands_catalog.clone(),
                model_id: params.model_id.or(session.model_id.clone()),
                supports_image_input: session.prompt_capabilities.image,
                preparation: TaskPreparationRecord::Ready,
            };

            let result = self.mutations.create_task_with_validation(
                record,
                loaded.replayed_messages,
                create_snapshot_commit_options(),
                |validation| {
                    validation.ensure_native_session_unowned(&selected_agent_id, &session_id)
                },
            )?;
            result
                .response_snapshot
                .ok_or_else(|| RuntimeError::Internal("missing task creation snapshot".to_string()))
        })();
        let snapshot = match persist_result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let session_id = session_start.session_id().to_string();
                let _ = session_start.close();
                let _ = self.fail_adopted_task_attach(&task_id, &session_id, &error);
                return Err(error);
            }
        };

        if let Err(error) = self
            .turn_runner
            .attach_session_events(task_id.clone(), &session_start.session().key())
        {
            let session_id = session_start.session_id().to_string();
            let _ = session_start.close();
            if let Err(finalize_error) =
                self.fail_adopted_task_attach(&task_id, &session_id, &error)
            {
                return Err(RuntimeError::Internal(format!(
                    "{error}; failed to finalize adopted task after session event attachment failure: {finalize_error}"
                )));
            }
            return Err(error);
        }
        let _session = session_start.commit();

        Ok(snapshot)
    }
}
