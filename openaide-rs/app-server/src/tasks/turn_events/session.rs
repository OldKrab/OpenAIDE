use crate::agent::{
    AgentMetadataField, AgentSessionEventSink, AgentSessionMetadataUpdate, TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};
use crate::time::now_string;

use super::commands::{update_task_commands, CommandsUpdateTarget};
use super::config::{update_task_config_options, ConfigUpdateTarget};
use super::{CatalogUpdateSource, TaskSessionEventSink};

impl AgentSessionEventSink for TaskSessionEventSink {
    fn config_options_changed(
        &self,
        catalog: crate::protocol::model::ConfigOptionsCatalog,
    ) -> Result<(), RuntimeError> {
        update_task_config_options(
            ConfigUpdateTarget {
                mutations: &self.mutations,
                task_id: &self.task_id,
            },
            catalog,
            &now_string(),
            CatalogUpdateSource::BoundSession {
                session_id: &self.session_id,
            },
        )
    }

    fn commands_changed(
        &self,
        catalog: crate::protocol::model::AgentCommandsCatalog,
    ) -> Result<(), RuntimeError> {
        update_task_commands(
            CommandsUpdateTarget {
                mutations: &self.mutations,
                task_id: &self.task_id,
            },
            catalog,
            &now_string(),
            CatalogUpdateSource::BoundSession {
                session_id: &self.session_id,
            },
        )
    }

    fn metadata_changed(&self, update: AgentSessionMetadataUpdate) -> Result<(), RuntimeError> {
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                let mut changed = false;
                match &update.title {
                    AgentMetadataField::Unchanged => {}
                    AgentMetadataField::Clear => changed |= task.agent_title.take().is_some(),
                    AgentMetadataField::Value(title) => {
                        let next = (!title.trim().is_empty()).then(|| title.trim().to_string());
                        if task.agent_title != next {
                            task.agent_title = next;
                            changed = true;
                        }
                    }
                }
                if let AgentMetadataField::Value(updated_at) = &update.updated_at {
                    let updated_at = updated_at.trim();
                    if !updated_at.is_empty() && task.last_activity != updated_at {
                        task.last_activity = updated_at.to_string();
                        task.updated_at = updated_at.to_string();
                        changed = true;
                    }
                }
                Ok(if changed {
                    TaskMutationResult::Changed
                } else {
                    TaskMutationResult::Unchanged
                })
            },
        )?;
        Ok(())
    }

    fn request_question(
        &self,
        form: openaide_app_server_protocol::server_requests::QuestionRequestParams,
        cancellation: TurnCancellation,
    ) -> Result<openaide_app_server_protocol::server_requests::QuestionRequestResponse, RuntimeError>
    {
        self.handle_question(form, cancellation)
    }

    fn record_question_error(&self, message: String) -> Result<(), RuntimeError> {
        self.append_question_error(message)
    }
}
