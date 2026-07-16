use uuid::Uuid;

use crate::agent::{AgentSessionStart, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{NormalizedMessage, TaskSnapshot, TaskStatus};
use crate::protocol::params::TaskCreateParams;
use crate::storage::records::TaskPreparationRecord;
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::tasks::config_options::selected_config_options;
use crate::tasks::lifecycle::running_turn_message;
use crate::tasks::task_start_transaction::TaskSessionStartGuard;
use crate::time::now_string;

use super::helpers::required_optional_prompt_text;
use super::{create_snapshot_commit_options, TaskTurnLifecycle};

impl TaskTurnLifecycle {
    pub(in crate::tasks::turn_lifecycle) fn create_prompt_start(
        &self,
        params: TaskCreateParams,
    ) -> Result<TaskSnapshot, RuntimeError> {
        self.agent_registry.validate_task_create(&params)?;
        let prompt_text = required_optional_prompt_text(params.prompt_text.clone(), "prompt_text")?;
        let selected_config_options = selected_config_options(params.config_options.as_ref())?;
        let now = now_string();
        let task_id = format!("task_{}", Uuid::new_v4());
        let prompt_attachments = params.context.clone();
        let turn_id = Uuid::new_v4().to_string();
        let mut session_start = TaskSessionStartGuard::new(
            &self.agent_gateway,
            self.agent_gateway.start_session(AgentSessionStart {
                agent_id: params.selected_agent_id.clone(),
                task_id: task_id.clone(),
                cwd: params.workspace_root.clone(),
                model_id: params.model_id.clone(),
                config_options: serde_json::to_value(&selected_config_options)
                    .ok()
                    .filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty)),
                config_option_policy: crate::agent::ConfigOptionPolicy::Strict,
                context: params.context.clone(),
                cancellation: TurnCancellation::new(),
                secret_resolver: None,
            })?,
        );

        let snapshot = {
            let session = session_start.session();
            let record = TaskRecord {
                task_id: task_id.clone(),
                title: None,
                status: TaskStatus::Active,
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
                active_turn_id: Some(turn_id.clone()),
                active_turn_started_at: Some(now.clone()),
                archived: false,
                tombstoned: false,
                revision: 0,
                config_options: session.config_options.clone(),
                config_options_catalog: session.config_catalog.clone(),
                config_mutation: Default::default(),
                agent_commands_catalog: None,
                model_id: params.model_id.or(session.model_id.clone()),
                supports_image_input: session.prompt_capabilities.image,
                preparation: TaskPreparationRecord::Ready,
            };
            let result = self.mutations.create_task(
                record,
                vec![
                    NormalizedMessage::User {
                        id: Uuid::new_v4().to_string(),
                        text: prompt_text.clone(),
                        created_at: now.clone(),
                        attachments: params.context,
                    },
                    running_turn_message(&now),
                ],
                create_snapshot_commit_options(),
            )?;
            result.response_snapshot.ok_or_else(|| {
                RuntimeError::Internal("missing task creation snapshot".to_string())
            })?
        };

        let session_sink = match self
            .turn_runner
            .attach_session_events(task_id.clone(), &session_start.session().key())
        {
            Ok(sink) => sink,
            Err(error) => {
                let session_id = session_start.session_id().to_string();
                let _ = session_start.close();
                if let Err(finalize_error) = self.fail_created_task_start(&task_id, &error) {
                    return Err(RuntimeError::Internal(format!(
                        "{error}; failed to finalize task after session event attachment failure for {session_id}: {finalize_error}"
                    )));
                }
                return Err(error);
            }
        };
        let session = session_start.commit();
        self.turn_runner.spawn_agent_turn(
            task_id.clone(),
            prompt_text,
            prompt_attachments,
            turn_id,
            session,
            session_sink,
        );
        Ok(snapshot)
    }
}
