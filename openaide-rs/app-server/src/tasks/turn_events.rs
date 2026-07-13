mod commands;
mod config;
mod permissions;
mod questions;
mod session;
#[cfg(test)]
mod tests;
mod text_chunks;

use crate::agent::events::{AgentEvent, AgentPermissionOutcome, AgentPermissionRequest};
use crate::agent::normalizer::normalize_event;
use crate::agent::{AgentEventSink, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;
use crate::server_requests::ServerRequestRuntime;
use crate::snapshots::task_snapshot::{project_chat_item, project_tool_details};
use crate::task_events::{CommittedTaskDelta, ToolDetailUpdate};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult, TaskMutations};
use crate::time::now_string;
use openaide_app_server_protocol::events::TextChunk;

use self::commands::{update_task_commands, CommandsUpdateTarget};
use self::config::{update_task_config_options, ConfigUpdateTarget};
use self::text_chunks::{TextChannel, TextChunkRoutes};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum CatalogUpdateSource<'a> {
    BoundSession { session_id: &'a str },
}

impl CatalogUpdateSource<'_> {
    fn matches(self, task: &crate::storage::records::TaskRecord) -> bool {
        match self {
            Self::BoundSession { session_id } => {
                task.agent_session_id.as_deref() == Some(session_id)
            }
        }
    }
}

pub(crate) struct TaskEventSink {
    mutations: TaskMutations,
    task_id: String,
    turn_id: String,
    session_sink: Arc<TaskSessionEventSink>,
    server_requests: ServerRequestRuntime,
    cancellation: TurnCancellation,
}

impl TaskEventSink {
    pub(crate) fn with_session_sink(
        mutations: TaskMutations,
        task_id: String,
        turn_id: String,
        session_sink: Arc<TaskSessionEventSink>,
        server_requests: ServerRequestRuntime,
        cancellation: TurnCancellation,
    ) -> Self {
        Self {
            mutations,
            task_id,
            turn_id,
            session_sink,
            server_requests,
            cancellation,
        }
    }

    #[cfg(test)]
    pub(crate) fn new(
        mutations: TaskMutations,
        task_id: String,
        turn_id: String,
        server_requests: ServerRequestRuntime,
        cancellation: TurnCancellation,
    ) -> Self {
        let session_sink = Arc::new(TaskSessionEventSink::new(
            mutations.clone(),
            task_id.clone(),
            "session_1".to_string(),
            server_requests.clone(),
        ));
        Self::with_session_sink(
            mutations,
            task_id,
            turn_id,
            session_sink,
            server_requests,
            cancellation,
        )
    }
}

pub(crate) struct TaskSessionEventSink {
    mutations: TaskMutations,
    task_id: String,
    session_id: String,
    server_requests: ServerRequestRuntime,
    text_chunk_routes: TextChunkRoutes,
    emission_lock: Mutex<()>,
}

impl TaskSessionEventSink {
    pub(crate) fn new(
        mutations: TaskMutations,
        task_id: String,
        session_id: String,
        server_requests: ServerRequestRuntime,
    ) -> Self {
        Self {
            mutations,
            task_id,
            session_id: session_id.clone(),
            server_requests,
            text_chunk_routes: TextChunkRoutes::new(session_id),
            emission_lock: Mutex::new(()),
        }
    }
}

impl AgentEventSink for TaskEventSink {
    fn emit(&self, event: AgentEvent) -> Result<(), RuntimeError> {
        self.session_sink.handle_session_update(event)
    }

    fn request_permission(
        &self,
        request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        self.handle_permission_request(request)
    }
}

impl TaskSessionEventSink {
    fn handle_session_update(&self, mut event: AgentEvent) -> Result<(), RuntimeError> {
        let _guard = self.emission_lock.lock().expect("event sink lock poisoned");
        let now = now_string();
        if let AgentEvent::ConfigOptionsChanged(catalog) = event {
            self.finish_anonymous_text_routes();
            return self.update_task_config_options(catalog, &now);
        }
        if let AgentEvent::CommandsChanged(catalog) = event {
            self.finish_anonymous_text_routes();
            return self.update_task_commands(catalog, &now);
        }
        if let AgentEvent::Text(text) = event {
            self.finish_anonymous_thought_run();
            return self.append_agent_text_chunk(text, None, &now);
        }
        if let AgentEvent::TextChunk {
            text,
            source_message_id,
        } = event
        {
            self.finish_anonymous_thought_run();
            return self.append_agent_text_chunk(text, source_message_id, &now);
        }
        if let AgentEvent::Thought(text) = event {
            self.finish_anonymous_text_run();
            return self.append_agent_thought_chunk(text, None, &now);
        }
        if let AgentEvent::ThoughtChunk {
            text,
            source_message_id,
        } = event
        {
            self.finish_anonymous_text_run();
            return self.append_agent_thought_chunk(text, source_message_id, &now);
        }
        self.finish_anonymous_text_routes();
        if let AgentEvent::ToolCall(tool_call) = &mut event {
            tool_call
                .scope_id
                .get_or_insert_with(|| self.session_id.clone());
            return self.upsert_session_tool(normalize_event(event, &now), &now);
        }
        self.append_session_message(normalize_event(event, &now), &now)
    }

    fn append_agent_text_chunk(
        &self,
        text: String,
        source_message_id: Option<String>,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let message_id = self
            .text_chunk_routes
            .message_id(TextChannel::Agent, source_message_id);
        self.commit_text_chunk(TextChannel::Agent, message_id, text, now)
    }

    fn finish_anonymous_text_run(&self) {
        self.text_chunk_routes.finish_anonymous(TextChannel::Agent);
    }

    fn append_agent_thought_chunk(
        &self,
        text: String,
        source_message_id: Option<String>,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let message_id = self
            .text_chunk_routes
            .message_id(TextChannel::Thought, source_message_id);
        self.commit_text_chunk(TextChannel::Thought, message_id, text, now)
    }

    fn finish_anonymous_thought_run(&self) {
        self.text_chunk_routes
            .finish_anonymous(TextChannel::Thought);
    }

    /// Sourced messages need no inferred lifetime. Only anonymous ACP chunks need
    /// a boundary when another content kind is observed.
    fn finish_anonymous_text_routes(&self) {
        self.text_chunk_routes.finish_all_anonymous();
    }

    fn commit_text_chunk(
        &self,
        channel: TextChannel,
        message_id: String,
        text: String,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let delta_text = text.clone();
        let message = match channel {
            TextChannel::Agent => NormalizedMessage::AgentText {
                id: message_id,
                text,
                created_at: now.to_string(),
            },
            TextChannel::Thought => NormalizedMessage::Thought {
                id: message_id,
                text,
                created_at: now.to_string(),
            },
        };
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
                let delta = match ctx.append_text_chunk(message)? {
                    crate::storage::message_store::TextChunkAppend::Appended(stored) => {
                        CommittedTaskDelta::ChatItemAppended {
                            item: project_chat_item(&stored.chat),
                        }
                    }
                    crate::storage::message_store::TextChunkAppend::Updated(stored) => {
                        CommittedTaskDelta::ChatItemChunk {
                            message_id: stored.chat.message_id.clone().into(),
                            chunk: TextChunk { text: delta_text },
                        }
                    }
                };
                ctx.set_committed_delta(delta);
                ctx.task_mut().updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    fn update_task_commands(
        &self,
        catalog: crate::protocol::model::AgentCommandsCatalog,
        now: &str,
    ) -> Result<(), RuntimeError> {
        update_task_commands(
            CommandsUpdateTarget {
                mutations: &self.mutations,
                task_id: &self.task_id,
            },
            catalog,
            now,
            CatalogUpdateSource::BoundSession {
                session_id: &self.session_id,
            },
        )
    }

    fn append_session_message(
        &self,
        message: NormalizedMessage,
        now: &str,
    ) -> Result<(), RuntimeError> {
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

                ctx.append_message(message)?;
                let task = ctx.task_mut();
                task.updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    fn upsert_session_tool(
        &self,
        message: NormalizedMessage,
        now: &str,
    ) -> Result<(), RuntimeError> {
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
                let upserted = ctx.upsert_message_with_details(message)?;
                ctx.set_committed_delta(CommittedTaskDelta::ChatItemUpserted {
                    item: project_chat_item(&upserted.stored.chat),
                    tool_details: upserted
                        .tool_details
                        .into_iter()
                        .map(|detail| ToolDetailUpdate {
                            artifact_id: detail.artifact_id,
                            details: project_tool_details(&detail.details),
                        })
                        .collect(),
                });
                ctx.task_mut().updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    fn update_task_config_options(
        &self,
        catalog: crate::protocol::model::ConfigOptionsCatalog,
        now: &str,
    ) -> Result<(), RuntimeError> {
        update_task_config_options(
            ConfigUpdateTarget {
                mutations: &self.mutations,
                task_id: &self.task_id,
            },
            catalog,
            now,
            CatalogUpdateSource::BoundSession {
                session_id: &self.session_id,
            },
        )
    }
}
