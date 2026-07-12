mod commands;
mod config;
mod permissions;
mod questions;
mod session;
mod streaming;
#[cfg(test)]
mod tests;

use crate::agent::events::{AgentEvent, AgentPermissionOutcome, AgentPermissionRequest};
use crate::agent::normalizer::normalize_event;
use crate::agent::{AgentEventSink, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{NormalizedMessage, TaskStatus};
use crate::server_requests::ServerRequestRuntime;
use crate::snapshots::task_snapshot::project_chat_item;
use crate::task_events::CommittedTaskDelta;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult, TaskMutations};
use crate::time::now_string;

use self::commands::{update_task_commands, CommandsUpdateTarget};
use self::config::{update_task_config_options, ConfigUpdateTarget};
use self::streaming::{StreamingDelta, StreamingRuns, StreamingWrite};
use std::sync::Mutex;

#[derive(Clone, Copy)]
enum CatalogUpdateSource<'a> {
    ActiveTurn {
        turn_id: &'a str,
        cancellation: &'a TurnCancellation,
    },
    BoundSession {
        session_id: &'a str,
    },
}

impl CatalogUpdateSource<'_> {
    fn matches(self, task: &crate::storage::records::TaskRecord) -> bool {
        match self {
            Self::ActiveTurn {
                turn_id,
                cancellation,
            } => task.active_turn_id.as_deref() == Some(turn_id) && !cancellation.is_cancelled(),
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
    streaming_runs: StreamingRuns,
    server_requests: ServerRequestRuntime,
    cancellation: TurnCancellation,
    emission_lock: Mutex<()>,
}

impl TaskEventSink {
    pub(crate) fn new(
        mutations: TaskMutations,
        task_id: String,
        turn_id: String,
        server_requests: ServerRequestRuntime,
        cancellation: TurnCancellation,
    ) -> Self {
        Self {
            mutations,
            task_id,
            turn_id,
            streaming_runs: StreamingRuns::default(),
            server_requests,
            cancellation,
            emission_lock: Mutex::new(()),
        }
    }

    pub(crate) fn finish(&self) -> Result<(), RuntimeError> {
        let _guard = self.emission_lock.lock().expect("event sink lock poisoned");
        self.finish_streaming_runs(&now_string())
    }
}

pub(crate) struct TaskSessionEventSink {
    mutations: TaskMutations,
    task_id: String,
    session_id: String,
    server_requests: ServerRequestRuntime,
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
            session_id,
            server_requests,
        }
    }
}

impl AgentEventSink for TaskEventSink {
    fn emit(&self, mut event: AgentEvent) -> Result<(), RuntimeError> {
        let _guard = self.emission_lock.lock().expect("event sink lock poisoned");
        let now = now_string();
        if let AgentEvent::ConfigOptionsChanged(catalog) = event {
            self.finish_anonymous_streaming_runs(&now)?;
            return self.update_task_config_options(catalog, &now);
        }
        if let AgentEvent::CommandsChanged(catalog) = event {
            self.finish_anonymous_streaming_runs(&now)?;
            return self.update_task_commands(catalog, &now);
        }
        if let AgentEvent::Text(text) = event {
            self.finish_anonymous_thought_run(&now)?;
            return self.append_agent_text_chunk(text, None, &now);
        }
        if let AgentEvent::TextChunk {
            text,
            source_message_id,
        } = event
        {
            self.finish_anonymous_thought_run(&now)?;
            return self.append_agent_text_chunk(text, source_message_id, &now);
        }
        if let AgentEvent::Thought(text) = event {
            self.finish_anonymous_text_run(&now)?;
            return self.append_agent_thought_chunk(text, None, &now);
        }
        if let AgentEvent::ThoughtChunk {
            text,
            source_message_id,
        } = event
        {
            self.finish_anonymous_text_run(&now)?;
            return self.append_agent_thought_chunk(text, source_message_id, &now);
        }
        self.finish_anonymous_streaming_runs(&now)?;
        let write_mode = if let AgentEvent::ToolCall(tool_call) = &mut event {
            tool_call
                .scope_id
                .get_or_insert_with(|| self.turn_id.clone());
            MessageWriteMode::UpsertByIdentity
        } else {
            MessageWriteMode::Append
        };
        let message = normalize_event(event, &now);
        self.append_agent_message(message, &now, None, write_mode)
    }

    fn request_permission(
        &self,
        request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        self.handle_permission_request(request)
    }
}

impl TaskEventSink {
    fn append_agent_text_chunk(
        &self,
        text: String,
        source_message_id: Option<String>,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let writes = self
            .streaming_runs
            .agent_text_chunk(text, source_message_id, now)?;
        self.commit_streaming_writes(writes, now)
    }

    fn finish_text_run(&self, now: &str) -> Result<(), RuntimeError> {
        self.commit_streaming_writes(self.streaming_runs.finish_text(now), now)
    }

    fn finish_anonymous_text_run(&self, now: &str) -> Result<(), RuntimeError> {
        self.commit_optional_streaming_write(self.streaming_runs.finish_anonymous_text(now), now)
    }

    fn append_agent_thought_chunk(
        &self,
        text: String,
        source_message_id: Option<String>,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let writes = self
            .streaming_runs
            .thought_chunk(text, source_message_id, now)?;
        self.commit_streaming_writes(writes, now)
    }

    fn finish_thought_run(&self, now: &str) -> Result<(), RuntimeError> {
        self.commit_streaming_writes(self.streaming_runs.finish_thought(now), now)
    }

    fn finish_anonymous_thought_run(&self, now: &str) -> Result<(), RuntimeError> {
        self.commit_optional_streaming_write(self.streaming_runs.finish_anonymous_thought(now), now)
    }

    /// Source-correlated streams remain active for the whole turn. Only anonymous
    /// streams need inferred boundaries when another event kind is observed.
    fn finish_anonymous_streaming_runs(&self, now: &str) -> Result<(), RuntimeError> {
        self.finish_anonymous_text_run(now)?;
        self.finish_anonymous_thought_run(now)
    }

    fn finish_streaming_runs(&self, now: &str) -> Result<(), RuntimeError> {
        self.finish_text_run(now)?;
        self.finish_thought_run(now)
    }

    fn commit_optional_streaming_write(
        &self,
        write: Option<StreamingWrite>,
        now: &str,
    ) -> Result<(), RuntimeError> {
        match write {
            Some(write) => self.commit_streaming_write(write, now),
            None => Ok(()),
        }
    }

    fn commit_streaming_writes(
        &self,
        writes: Vec<StreamingWrite>,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let mut pending = writes.into_iter();
        while let Some(write) = pending.next() {
            if let Err(error) = self.commit_streaming_write(write, now) {
                // The writes were prepared as one in-memory transition. Undo
                // any later writes that persistence never had a chance to see.
                for uncommitted in pending {
                    self.streaming_runs.rollback(uncommitted);
                }
                return Err(error);
            }
        }
        Ok(())
    }

    fn commit_streaming_write(&self, write: StreamingWrite, now: &str) -> Result<(), RuntimeError> {
        let rollback = write.clone();
        let result = self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if ctx.task().active_turn_id.as_deref() != Some(self.turn_id.as_str())
                    || self.cancellation.is_cancelled()
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let stored = ctx.upsert_message_with_record(write.message)?;
                let delta = match write.delta {
                    StreamingDelta::Append => CommittedTaskDelta::ChatItemAppended {
                        item: project_chat_item(&stored.chat),
                    },
                    StreamingDelta::Chunk(chunk) => CommittedTaskDelta::ChatItemChunk {
                        message_id: stored.chat.message_id.clone().into(),
                        chunk,
                    },
                };
                ctx.set_committed_delta(delta);
                ctx.task_mut().updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        );
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                self.streaming_runs.rollback(rollback);
                Err(error)
            }
        }
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
            CatalogUpdateSource::ActiveTurn {
                turn_id: &self.turn_id,
                cancellation: &self.cancellation,
            },
        )
    }

    fn append_agent_message(
        &self,
        message: NormalizedMessage,
        now: &str,
        status: Option<TaskStatus>,
        write_mode: MessageWriteMode,
    ) -> Result<(), RuntimeError> {
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if ctx.task().active_turn_id.as_deref() != Some(self.turn_id.as_str())
                    || self.cancellation.is_cancelled()
                {
                    return Ok(TaskMutationResult::Unchanged);
                }

                match write_mode {
                    MessageWriteMode::Append => ctx.append_message(message)?,
                    MessageWriteMode::UpsertByIdentity => ctx.upsert_message(message)?,
                }
                if let Some(status) = status {
                    ctx.task_mut().status = status;
                }
                let task = ctx.task_mut();
                task.updated_at = now.to_string();
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
            CatalogUpdateSource::ActiveTurn {
                turn_id: &self.turn_id,
                cancellation: &self.cancellation,
            },
        )
    }
}

#[derive(Clone, Copy)]
enum MessageWriteMode {
    Append,
    UpsertByIdentity,
}
