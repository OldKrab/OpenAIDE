mod commands;
mod config;
mod permissions;
mod questions;
mod session;
#[cfg(test)]
mod tests;
mod text_chunks;

use crate::agent::events::{
    AgentContextUsage, AgentEvent, AgentPermissionOutcome, AgentPermissionRequest, AgentTurnUsage,
};
use crate::agent::normalizer::normalize_event;
use crate::agent::{AgentEventSink, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;
use crate::server_requests::ServerRequestRuntime;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult, TaskMutations};
use crate::time::now_string;

use self::commands::{update_task_commands, CommandsUpdateTarget};
use self::config::{update_task_config_options, ConfigUpdateTarget};
use self::text_chunks::{TextChannel, TextChunkRoutes};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
    native_catalog: Option<crate::native_sessions::catalog::NativeSessionCatalog>,
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
            native_catalog: None,
            server_requests,
            text_chunk_routes: TextChunkRoutes::new(session_id),
            emission_lock: Mutex::new(()),
        }
    }

    pub(crate) fn with_native_catalog(
        mut self,
        native_catalog: Option<crate::native_sessions::catalog::NativeSessionCatalog>,
    ) -> Self {
        self.native_catalog = native_catalog;
        self
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
        if let AgentEvent::ContextUsage(usage) = event {
            self.finish_anonymous_text_routes();
            return self.update_context_usage(usage);
        }
        if let AgentEvent::TurnUsage(usage) = event {
            self.finish_anonymous_text_routes();
            return self.update_turn_usage(usage);
        }
        if let AgentEvent::Plan(plan) = event {
            self.finish_anonymous_text_routes();
            return self.update_plan(plan, &now);
        }
        if let AgentEvent::MessageChunk {
            role,
            part,
            source_message_id,
        } = event
        {
            let channel = match role {
                crate::protocol::model::AgentMessageRole::Agent => TextChannel::Agent,
                crate::protocol::model::AgentMessageRole::Thought => TextChannel::Thought,
            };
            match channel {
                TextChannel::Agent => self.finish_anonymous_thought_run(),
                TextChannel::Thought => self.finish_anonymous_text_run(),
            }
            let anonymous_non_text = source_message_id.is_none()
                && !matches!(&part, crate::protocol::model::AgentMessagePart::Text { .. });
            let message_id = self
                .text_chunk_routes
                .message_id(channel, source_message_id);
            let result = self.commit_agent_message_part(role, message_id, part, &now);
            if anonymous_non_text {
                self.text_chunk_routes.finish_anonymous(channel);
            }
            return result;
        }
        self.finish_anonymous_text_routes();
        if let AgentEvent::ToolUpdate(mut update) = event {
            let terminal_appends = update
                .terminal_appends
                .into_iter()
                .map(|append| crate::storage::task_journal::ToolTerminalAppend {
                    artifact_id: crate::storage::tool_artifacts::tool_artifact_id(
                        &format!("acp_tool:{}:{}", self.session_id, append.tool_call_id),
                        0,
                    ),
                    terminal_id: append.terminal_id,
                    data: append.data,
                })
                .collect::<Vec<_>>();
            let Some(mut tool_call) = update.summary.take() else {
                if terminal_appends.is_empty() {
                    return Ok(());
                }
                self.mutations.append_terminal_outputs(
                    &self.task_id,
                    &self.session_id,
                    terminal_appends,
                )?;
                return Ok(());
            };
            tool_call.scope_id = Some(self.session_id.clone());
            let message = normalize_event(AgentEvent::ToolCall(tool_call), &now);
            return self.upsert_session_tool_with_terminal(message, terminal_appends, &now);
        }
        if let AgentEvent::ToolCall(tool_call) = &mut event {
            tool_call
                .scope_id
                .get_or_insert_with(|| self.session_id.clone());
            return self.upsert_session_tool(normalize_event(event, &now), &now);
        }
        if matches!(event, AgentEvent::Subagent(_)) {
            return self.upsert_session_tool(normalize_event(event, &now), &now);
        }
        self.append_session_message(normalize_event(event, &now), &now)
    }

    fn finish_anonymous_text_run(&self) {
        self.text_chunk_routes.finish_anonymous(TextChannel::Agent);
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

    fn update_plan(
        &self,
        plan: crate::protocol::model::AgentPlan,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let all_completed = !plan.entries.is_empty()
            && plan
                .entries
                .iter()
                .all(|entry| entry.status == crate::protocol::model::AgentPlanStatus::Completed);
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: all_completed,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                if plan.entries.is_empty() {
                    if ctx.task().current_plan.is_none()
                        && ctx.task().completed_plan_message_id.is_none()
                    {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    let task = ctx.task_mut();
                    task.current_plan = None;
                    task.completed_plan_message_id = None;
                    task.updated_at = now.to_string();
                    return Ok(TaskMutationResult::Changed);
                }
                if all_completed {
                    let message_id =
                        ctx.task()
                            .completed_plan_message_id
                            .clone()
                            .unwrap_or_else(|| {
                                format!("acp_plan:{}:{}", self.session_id, uuid::Uuid::new_v4())
                            });
                    ctx.upsert_message_with_details(NormalizedMessage::CompletedPlan {
                        id: message_id.clone(),
                        entries: plan.entries,
                        created_at: now.to_string(),
                    })?;
                    let task = ctx.task_mut();
                    task.current_plan = None;
                    task.completed_plan_message_id = Some(message_id);
                    task.updated_at = now.to_string();
                    return Ok(TaskMutationResult::Changed);
                }
                let next = Some(plan);
                if ctx.task().current_plan == next && ctx.task().completed_plan_message_id.is_none()
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                task.current_plan = next;
                task.completed_plan_message_id = None;
                task.updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    fn commit_agent_message_part(
        &self,
        role: crate::protocol::model::AgentMessageRole,
        message_id: String,
        part: crate::protocol::model::AgentMessagePart,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let text_bytes = match &part {
            crate::protocol::model::AgentMessagePart::Text { text } => Some(text.len()),
            _ => None,
        };
        if let crate::protocol::model::AgentMessagePart::Text { text } = &part {
            match self.mutations.stream_agent_message_text(
                &self.task_id,
                &self.session_id,
                role,
                &message_id,
                text,
                now,
            )? {
                crate::tasks::mutation::AgentMessageTextStreamOutcome::Admitted
                | crate::tasks::mutation::AgentMessageTextStreamOutcome::IgnoredStaleSession => {
                    return Ok(())
                }
                crate::tasks::mutation::AgentMessageTextStreamOutcome::NeedsMessageCommit => {}
            }
        }
        let message = NormalizedMessage::AgentMessage {
            id: message_id,
            role,
            parts: vec![part],
            created_at: now.to_string(),
        };
        let started = Instant::now();
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
                ctx.append_agent_message_part(message)?;
                ctx.task_mut().updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        let elapsed = started.elapsed();
        if elapsed.as_millis() >= 50 {
            crate::logging::warn(
                "agent_message_part_commit_slow",
                serde_json::json!({
                    "task_id": self.task_id,
                    "duration_ms": elapsed.as_millis(),
                    "text_bytes": text_bytes,
                }),
            );
        }
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
                ctx.upsert_message_with_details(message)?;
                ctx.task_mut().updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    fn upsert_session_tool_with_terminal(
        &self,
        message: NormalizedMessage,
        terminal_appends: Vec<crate::storage::task_journal::ToolTerminalAppend>,
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
                ctx.upsert_message_with_details(message)?;
                for append in terminal_appends {
                    ctx.append_terminal(append.artifact_id, append.terminal_id, append.data);
                }
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

    fn update_context_usage(&self, usage: AgentContextUsage) -> Result<(), RuntimeError> {
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let next = crate::protocol::model::TaskContextUsage {
                    used_tokens: usage.used_tokens,
                    capacity_tokens: usage.capacity_tokens,
                    cost: usage
                        .cost
                        .map(|cost| crate::protocol::model::TaskUsageCost {
                            amount: cost.amount,
                            currency: cost.currency,
                        }),
                    last_turn: None,
                };
                if ctx.task().context_usage.as_ref() == Some(&next) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.task_mut().context_usage = Some(next);
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    fn update_turn_usage(&self, usage: AgentTurnUsage) -> Result<(), RuntimeError> {
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let next = crate::protocol::model::TaskTurnUsage {
                    total_tokens: usage.total_tokens,
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    reasoning_tokens: usage.reasoning_tokens,
                    cached_read_tokens: usage.cached_read_tokens,
                    cached_write_tokens: usage.cached_write_tokens,
                };
                if ctx.task().last_turn_usage.as_ref() == Some(&next) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.task_mut().last_turn_usage = Some(next);
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }
}
