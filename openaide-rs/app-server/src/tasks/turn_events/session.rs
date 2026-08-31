use crate::agent::events::{
    AgentEvent, AgentNativeSubagentSpawned, AgentNativeSubagentState,
    AgentNativeSubagentStateUpdate,
};
use crate::agent::{
    AgentMetadataField, AgentSessionEventSink, AgentSessionMetadataUpdate, TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, ActivityStep, NormalizedMessage};
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

    fn subagent_spawned(&self, event: AgentNativeSubagentSpawned) -> Result<(), RuntimeError> {
        let _guard = self.emission_lock.lock().expect("event sink lock poisoned");
        let now = now_string();
        let prompt_unavailable = event.delegated_task.is_none();
        let child_already_exists = match self
            .mutations
            .store()
            .subagent_record_by_native(&self.task_id, &event.native_session_id)
        {
            Ok(_) => true,
            Err(RuntimeError::TaskNotFound(_)) => false,
            Err(error) => return Err(error),
        };
        // A resumed Codex connection has a fresh in-memory router, so its first
        // repeated announcement cannot be classified there. Codex omits the
        // delegated prompt, while the durable child identity proves this is not
        // the child's first announcement.
        let parent_interaction =
            event.parent_interaction || (prompt_unavailable && child_already_exists);
        let actor = if event.parent_native_session_id == self.session_id {
            "Main Agent"
        } else {
            "Parent Agent"
        };
        let mut record = self.mutations.store().record_subagent_spawn(
            &self.task_id,
            &event.parent_native_session_id,
            &event.native_session_id,
            event.name,
            event.delegated_task.unwrap_or_default(),
            crate::storage::subagents::SubagentCapabilitiesRecord {
                cancel: event.capabilities.cancel,
                close: event.capabilities.close,
            },
            event
                .details
                .into_iter()
                .map(|detail| crate::storage::subagents::SubagentDetailRecord {
                    label: detail.label,
                    value: detail.value,
                })
                .collect(),
        )?;
        if parent_interaction {
            record = self.mutations.store().update_subagent_status(
                &self.task_id,
                &record.native_session_id,
                crate::storage::subagents::SubagentStatusRecord::WaitingForActivity,
            )?;
            self.mutations.store().append_subagent_message(
                &self.task_id,
                &record.native_session_id,
                codex_subagent_notice(
                    format!(
                        "{}:codex:interaction:{}",
                        record.subagent_id,
                        record.history_revision.saturating_add(1),
                    ),
                    &format!("{actor} interacted with this subagent"),
                    "The inter-agent message is unavailable because Codex does not expose it to clients.",
                    &now,
                ),
                false,
            )?;
        } else if prompt_unavailable {
            self.mutations.store().append_subagent_message(
                &self.task_id,
                &record.native_session_id,
                codex_subagent_notice(
                    format!("{}:codex:started", record.subagent_id),
                    &format!("{actor} started this subagent"),
                    "The original delegated prompt is unavailable because Codex does not expose it to clients.",
                    &now,
                ),
                true,
            )?;
        }
        let identity = subagent_lifecycle_identity(&record.subagent_id);
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.upsert_message_with_details(NormalizedMessage::Activity {
                    id: identity.clone(),
                    title: format!("Delegated to {}", record.name),
                    status: ActivityStatus::Running,
                    created_at: now.clone(),
                    collapsed: true,
                    steps: vec![ActivityStep::Subagent {
                        subagent_id: Some(record.subagent_id.clone()),
                        tool_call_id: None,
                        title: (!record.delegated_task.is_empty())
                            .then_some(record.delegated_task.clone()),
                        thread_id: None,
                        raw_path: None,
                        activity: None,
                        name: record.name.clone(),
                        path: Vec::new(),
                        status: ActivityStatus::Running,
                        events: Vec::new(),
                    }],
                })?;
                ctx.task_mut().updated_at = now.clone();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        self.publish_subagent_snapshots(&record.subagent_id)
    }

    fn subagent_session_update(
        &self,
        native_session_id: &str,
        mut event: AgentEvent,
    ) -> Result<(), RuntimeError> {
        let _guard = self.emission_lock.lock().expect("event sink lock poisoned");
        let now = now_string();
        let record = self
            .mutations
            .store()
            .subagent_record_by_native(&self.task_id, native_session_id)?;
        let nested_target = if let AgentEvent::Subagent(subagent) = &event {
            self.append_codex_subagent_interaction(subagent, "Parent Agent", &now)?
        } else {
            None
        };
        match event {
            AgentEvent::UserMessageChunk {
                text,
                source_message_id,
            } => {
                let message_id = {
                    let mut routes = self
                        .subagent_text_chunk_routes
                        .lock()
                        .expect("Subagent text route lock poisoned");
                    routes
                        .entry(record.subagent_id.clone())
                        .or_insert_with(|| super::TextChunkRoutes::new(record.subagent_id.clone()))
                        .message_id(super::TextChannel::User, source_message_id)
                };
                self.mutations.store().append_subagent_message_part(
                    &self.task_id,
                    native_session_id,
                    NormalizedMessage::User {
                        id: message_id,
                        text,
                        created_at: now,
                        attachments: Vec::new(),
                    },
                )?;
            }
            AgentEvent::MessageChunk {
                role,
                part,
                source_message_id,
            } => {
                let channel = match role {
                    crate::protocol::model::AgentMessageRole::Agent => super::TextChannel::Agent,
                    crate::protocol::model::AgentMessageRole::Thought => {
                        super::TextChannel::Thought
                    }
                };
                let message_id = {
                    let mut routes = self
                        .subagent_text_chunk_routes
                        .lock()
                        .expect("Subagent text route lock poisoned");
                    routes
                        .entry(record.subagent_id.clone())
                        .or_insert_with(|| super::TextChunkRoutes::new(record.subagent_id.clone()))
                        .message_id(channel, source_message_id)
                };
                self.mutations.store().append_subagent_message_part(
                    &self.task_id,
                    native_session_id,
                    NormalizedMessage::AgentMessage {
                        id: message_id,
                        role,
                        parts: vec![part],
                        created_at: now,
                    },
                )?;
            }
            AgentEvent::Plan(plan) => {
                self.mutations
                    .store()
                    .set_subagent_plan(&self.task_id, native_session_id, plan)?;
            }
            AgentEvent::ToolUpdate(mut update) => {
                if let Some(mut tool) = update.summary.take() {
                    tool.scope_id = Some(record.subagent_id.clone());
                    let message =
                        crate::agent::normalizer::normalize_event(AgentEvent::ToolCall(tool), &now);
                    self.mutations.store().append_subagent_message(
                        &self.task_id,
                        native_session_id,
                        message,
                        true,
                    )?;
                }
            }
            AgentEvent::ToolCall(ref mut tool) => {
                tool.scope_id = Some(record.subagent_id.clone());
                let message = crate::agent::normalizer::normalize_event(event, &now);
                self.mutations.store().append_subagent_message(
                    &self.task_id,
                    native_session_id,
                    message,
                    true,
                )?;
            }
            AgentEvent::ConfigOptionsChanged(_)
            | AgentEvent::CommandsChanged(_)
            | AgentEvent::ContextUsage(_)
            | AgentEvent::TurnUsage(_) => {}
            other => {
                let message = crate::agent::normalizer::normalize_event(other, &now);
                self.mutations.store().append_subagent_message(
                    &self.task_id,
                    native_session_id,
                    message,
                    false,
                )?;
            }
        }
        self.publish_subagent_snapshots(&record.subagent_id)?;
        if let Some(target_subagent_id) = nested_target {
            if target_subagent_id != record.subagent_id {
                self.publish_subagent_snapshots(&target_subagent_id)?;
            }
        }
        Ok(())
    }

    fn subagent_state_changed(
        &self,
        event: AgentNativeSubagentStateUpdate,
    ) -> Result<(), RuntimeError> {
        let _guard = self.emission_lock.lock().expect("event sink lock poisoned");
        let (stored_status, activity_status) = match event.state {
            AgentNativeSubagentState::Completed => (
                crate::storage::subagents::SubagentStatusRecord::Completed,
                ActivityStatus::Completed,
            ),
            AgentNativeSubagentState::Failed => (
                crate::storage::subagents::SubagentStatusRecord::Failed,
                ActivityStatus::Error,
            ),
            AgentNativeSubagentState::Cancelled => (
                crate::storage::subagents::SubagentStatusRecord::Cancelled,
                ActivityStatus::Interrupted,
            ),
            AgentNativeSubagentState::Disconnected => (
                crate::storage::subagents::SubagentStatusRecord::Disconnected,
                ActivityStatus::Interrupted,
            ),
        };
        let record = self.mutations.store().update_subagent_status(
            &self.task_id,
            &event.native_session_id,
            stored_status,
        )?;
        let identity = subagent_lifecycle_identity(&record.subagent_id);
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if ctx.finish_running_activity(&identity, activity_status)? {
                    ctx.task_mut().updated_at = now_string();
                    Ok(TaskMutationResult::Changed)
                } else {
                    Ok(TaskMutationResult::Unchanged)
                }
            },
        )?;
        self.publish_subagent_snapshots(&record.subagent_id)
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

impl TaskSessionEventSink {
    pub(super) fn append_codex_subagent_interaction(
        &self,
        subagent: &crate::agent::events::AgentSubagent,
        actor: &str,
        now: &str,
    ) -> Result<Option<String>, RuntimeError> {
        if subagent.activity != "interacted" {
            return Ok(None);
        }
        let record = self
            .mutations
            .store()
            .subagent_record_by_native(&self.task_id, &subagent.thread_id)?;
        self.mutations.store().append_subagent_message(
            &self.task_id,
            &record.native_session_id,
            codex_subagent_notice(
                format!(
                    "{}:codex:interaction:{}",
                    record.subagent_id, subagent.tool_call_id
                ),
                &format!("{actor} interacted with this subagent"),
                "The inter-agent message is unavailable because Codex does not expose it to clients.",
                now,
            ),
            true,
        )?;
        Ok(Some(record.subagent_id))
    }
}

fn codex_subagent_notice(
    id: String,
    title: &str,
    explanation: &str,
    created_at: &str,
) -> NormalizedMessage {
    NormalizedMessage::Activity {
        id,
        title: title.to_string(),
        status: ActivityStatus::Completed,
        created_at: created_at.to_string(),
        collapsed: true,
        steps: vec![ActivityStep::Text {
            text: explanation.to_string(),
            // Frontend renders agent-boundary notices separately from ordinary Tool activity.
            level: Some("agent_boundary".to_string()),
        }],
    }
}

impl TaskSessionEventSink {
    pub(super) fn publish_subagent_snapshots(&self, subagent_id: &str) -> Result<(), RuntimeError> {
        let task_id = openaide_app_server_protocol::ids::TaskId::from(self.task_id.clone());
        let catalog = crate::snapshots::task_snapshot::project_subagent_catalog(
            task_id.clone(),
            self.mutations.store().subagent_catalog(&self.task_id),
        )
        .map_err(|error| RuntimeError::Internal(error.message))?;
        self.mutations.publish_subagent_catalog(catalog);

        let history = self
            .mutations
            .store()
            .subagent_history(&self.task_id, subagent_id)?;
        let items = history
            .messages
            .iter()
            .map(|stored| crate::snapshots::task_snapshot::project_chat_item(&stored.chat))
            .collect::<Vec<_>>();
        let start_cursor = history
            .messages
            .first()
            .map(|stored| stored.chat.cursor.clone().into());
        let end_cursor = history
            .messages
            .last()
            .map(|stored| stored.chat.cursor.clone().into());
        self.mutations.publish_subagent_history(
            openaide_app_server_protocol::snapshot::SubagentHistorySnapshot {
                task_id,
                subagent_id: subagent_id.to_string().into(),
                revision: history.revision,
                availability: if history.messages.is_empty() {
                    openaide_app_server_protocol::snapshot::SubagentHistoryAvailability::WaitingForActivity
                } else {
                    openaide_app_server_protocol::snapshot::SubagentHistoryAvailability::Available
                },
                chat: openaide_app_server_protocol::snapshot::ChatSnapshot {
                    has_messages: !items.is_empty(),
                    items,
                    has_more_before: false,
                    start_cursor: start_cursor.clone(),
                    end_cursor,
                },
                current_plan: history
                    .current_plan
                    .map(crate::snapshots::task_snapshot::project_agent_plan),
                start_cursor,
            },
        );
        Ok(())
    }
}

fn subagent_lifecycle_identity(subagent_id: &str) -> String {
    format!("subagent_lifecycle:{subagent_id}")
}
