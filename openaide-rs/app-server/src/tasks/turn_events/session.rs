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
    fn session_update(&self, event: crate::agent::events::AgentEvent) -> Result<(), RuntimeError> {
        self.handle_session_update(event)
    }

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
        let catalog_title = match &update.title {
            AgentMetadataField::Unchanged => None,
            AgentMetadataField::Clear => Some(None),
            AgentMetadataField::Value(title) => Some(Some(title.clone())),
        };
        let catalog_updated_at = match &update.updated_at {
            AgentMetadataField::Value(updated_at) => Some(updated_at.trim().to_string()),
            AgentMetadataField::Unchanged | AgentMetadataField::Clear => None,
        };
        let mut catalog_reference = None;
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                catalog_reference = Some(crate::native_sessions::catalog::NativeSessionRef::new(
                    &ctx.task().agent_id,
                    &self.session_id,
                ));
                let task = ctx.task_mut();
                let mut changed = false;
                match &update.title {
                    AgentMetadataField::Unchanged => {}
                    AgentMetadataField::Clear => changed |= task.clear_agent_title(),
                    AgentMetadataField::Value(title) => changed |= task.set_agent_title(title),
                }
                if let AgentMetadataField::Value(updated_at) = &update.updated_at {
                    let updated_at = updated_at.trim();
                    let advances_activity = crate::time::activity_millis(updated_at)
                        .zip(crate::time::activity_millis(&task.last_activity))
                        .is_some_and(|(native, task)| native > task);
                    if advances_activity {
                        task.last_activity = updated_at.to_string();
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
        if let (Some(catalog), Some(reference)) = (&self.native_catalog, catalog_reference) {
            if let Err(error) =
                catalog.record_live_metadata(&reference, catalog_title, catalog_updated_at)
            {
                // Task metadata is authoritative for an owned session. A secondary catalog
                // persistence failure must not detach its live update consumer.
                crate::logging::warn(
                    "native_session_catalog_live_metadata_failed",
                    serde_json::json!({
                        "task_id": self.task_id,
                        "agent_id": reference.agent_id,
                        "session_id": reference.session_id,
                        "error_code": error.code(),
                    }),
                );
            }
        }
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
