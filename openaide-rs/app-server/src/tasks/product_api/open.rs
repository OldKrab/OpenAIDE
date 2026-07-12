use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::{
    TaskMarkReadParams, TaskOpenParams, TaskRetryHistorySyncParams,
};
use std::time::Instant;

use crate::agent::{
    AgentListSessionsRequest, AgentSessionLoad, AgentSessionResume, TurnCancellation,
};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentListedSession, TaskStatus as LegacyTaskStatus};
use crate::snapshots::task_snapshot::project_stored_task_snapshot;
use crate::storage::records::TaskRecord;
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::tasks::task_start_transaction::TaskSessionStartGuard;

use super::{internal_error, protocol_error_from_runtime, runtime_error, TaskProductApi};

pub(crate) trait TaskOpenWorkflow: Send + Sync {
    fn open(&self, params: TaskOpenParams) -> Result<TaskSnapshot, ProtocolError>;
    fn mark_read(&self, params: TaskMarkReadParams) -> Result<TaskSnapshot, ProtocolError>;
    fn retry_history_sync(
        &self,
        params: TaskRetryHistorySyncParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.open(TaskOpenParams {
            task_id: params.task_id,
        })
    }
}

impl TaskProductApi {
    pub(super) fn mark_task_read(
        &self,
        params: TaskMarkReadParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let result = self
            .mutations
            .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                if ctx.task().tombstoned {
                    return Err(RuntimeError::TaskNotFound(task_id.clone()));
                }
                if !ctx.task().unread {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.task_mut().unread = false;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing task mark-read snapshot"))?;
        project_stored_task_snapshot(snapshot)
    }

    pub(super) fn open_task(&self, params: TaskOpenParams) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let task = self.store.read_task(&task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&task)?;

        let result = self
            .mutations
            .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                if ctx.task().tombstoned {
                    return Err(RuntimeError::TaskNotFound(task_id.clone()));
                }
                if !ctx.task().unread {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.task_mut().unread = false;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing task open snapshot"))?;
        self.spawn_adopted_task_refresh(task);
        project_stored_task_snapshot(snapshot)
    }

    /// Native history reconciliation may involve a slow Agent operation. Task opening is
    /// cache-first; any fresher history is committed and published after the response.
    fn spawn_adopted_task_refresh(&self, task: TaskRecord) {
        let generation = self.history_sync.begin_passive(&task.task_id);
        self.publish_history_sync(
            &task.task_id,
            openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Checking {
                generation: generation.value(),
            },
        );
        let api = self.clone();
        std::thread::spawn(move || {
            let result = api.refresh_adopted_task_from_native_session_if_newer(&task, generation);
            if !api.history_sync.is_current(&task.task_id, generation) {
                return;
            }
            match result {
                Ok(Some(_)) => api.publish_history_sync(
                    &task.task_id,
                    openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Updated {
                        generation: generation.value(),
                    },
                ),
                Ok(None) => api.publish_history_sync(
                    &task.task_id,
                    openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle {
                        generation: generation.value(),
                    },
                ),
                Err(error) => {
                    api.publish_history_sync(
                        &task.task_id,
                        openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Failed {
                            generation: generation.value(),
                            message: error.message.clone(),
                            before_send: false,
                        },
                    );
                    logging::warn(
                        "adopted_task_background_refresh_failed",
                        serde_json::json!({
                            "task_id": task.task_id,
                            "agent_id": task.agent_id,
                            "error": error.message,
                        }),
                    );
                }
            }
        });
    }

    fn refresh_adopted_task_from_native_session_if_newer(
        &self,
        task: &TaskRecord,
        generation: crate::tasks::history_sync::PassiveSyncGeneration,
    ) -> Result<Option<crate::protocol::model::TaskSnapshot>, ProtocolError> {
        if !task.first_prompt_sent
            || task.status == LegacyTaskStatus::Active
            || task.active_turn_id.is_some()
        {
            return Ok(None);
        }
        let Some(stored_session_id) = task.agent_session_id.clone() else {
            return Ok(None);
        };
        if self.native_session_is_active(task, &stored_session_id)? {
            return Ok(None);
        }
        let Some(native_session) = self.native_session_for_task(task, &stored_session_id)? else {
            return Ok(None);
        };
        self.history_sync
            .run_passive(&task.task_id, generation, || {
                self.refresh_adopted_task_from_native_session_if_newer_exclusive(
                    task,
                    stored_session_id,
                    native_session,
                )
            })
            .unwrap_or(Ok(None))
    }

    fn refresh_adopted_task_from_native_session_if_newer_exclusive(
        &self,
        task: &TaskRecord,
        stored_session_id: String,
        native_session: AgentListedSession,
    ) -> Result<Option<crate::protocol::model::TaskSnapshot>, ProtocolError> {
        let current_task = self.store.read_task(&task.task_id).map_err(runtime_error)?;
        if current_task.agent_session_id.as_deref() != Some(stored_session_id.as_str())
            || self.native_session_is_active(&current_task, &stored_session_id)?
        {
            return Ok(None);
        }
        let refresh_started = Instant::now();
        let load_started = Instant::now();
        let loaded = self
            .agent_gateway
            .load_session(AgentSessionLoad {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                session_id: stored_session_id.clone(),
                cancellation: TurnCancellation::new(),
                secret_resolver: Some(self.task_secret_resolver(&task.task_id)),
            })
            .map_err(protocol_error_from_runtime)?;
        let load_ms = load_started.elapsed().as_millis();
        let mut session_start =
            TaskSessionStartGuard::new(&self.agent_gateway, loaded.session.clone());
        let loaded_session_id = session_start.session_id().to_string();
        let refreshed_at = native_session
            .last_activity
            .clone()
            .unwrap_or_else(crate::time::now_string);
        let refreshed_title = native_session
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string);
        let config_options = loaded.session.config_options.clone();
        let config_options_catalog = loaded.session.config_catalog.clone();
        let agent_commands_catalog = loaded.session.commands_catalog.clone();
        let model_id = loaded.session.model_id.clone();
        let replayed_messages = loaded.replayed_messages;
        let replayed_message_count = replayed_messages.len();

        let commit_started = Instant::now();
        let result = self
            .mutations
            .commit_existing_task(
                &task.task_id,
                TaskCommitOptions {
                    refresh_message_history: true,
                    response_snapshot_tail_limit: Some(100),
                },
                |ctx| {
                    if ctx.task().agent_session_id.as_deref() != Some(stored_session_id.as_str())
                        || ctx.task().status == LegacyTaskStatus::Active
                        || ctx.task().active_turn_id.is_some()
                    {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    ctx.replace_messages(replayed_messages)?;
                    let task = ctx.task_mut();
                    if let Some(title) = refreshed_title {
                        task.agent_title = Some(title);
                    }
                    task.status = LegacyTaskStatus::Inactive;
                    task.unread = false;
                    task.first_prompt_sent = true;
                    task.agent_session_id = Some(loaded_session_id.clone());
                    task.config_options = config_options;
                    task.config_options_catalog = config_options_catalog;
                    task.agent_commands_catalog = agent_commands_catalog;
                    task.model_id = model_id;
                    task.updated_at = refreshed_at.clone();
                    task.last_activity = refreshed_at;
                    Ok(TaskMutationResult::Changed)
                },
            )
            .map_err(protocol_error_from_runtime)?;
        let commit_ms = commit_started.elapsed().as_millis();

        let snapshot = match result.outcome {
            TaskCommitOutcome::Committed(_) => result
                .response_snapshot
                .ok_or_else(|| internal_error("missing refreshed task snapshot"))?,
            TaskCommitOutcome::Rejected(_) => {
                let _ = session_start.close();
                return Ok(None);
            }
        };

        let attach_started = Instant::now();
        if let Err(error) = self
            .turn_runner
            .attach_session_events(task.task_id.clone(), &loaded_session_id)
        {
            let _ = session_start.close();
            return Err(protocol_error_from_runtime(error));
        }
        let attach_ms = attach_started.elapsed().as_millis();
        let _session = session_start.commit();
        logging::info(
            "adopted_task_refresh_timing",
            serde_json::json!({
                "task_id": task.task_id,
                "agent_id": task.agent_id,
                "message_count": replayed_message_count,
                "load_ms": load_ms,
                "commit_ms": commit_ms,
                "attach_ms": attach_ms,
                "total_ms": refresh_started.elapsed().as_millis(),
            }),
        );
        Ok(Some(snapshot))
    }

    fn native_session_is_active(
        &self,
        task: &TaskRecord,
        session_id: &str,
    ) -> Result<bool, ProtocolError> {
        match self.agent_gateway.resume_session(AgentSessionResume {
            agent_id: task.agent_id.clone(),
            task_id: task.task_id.clone(),
            session_id: session_id.to_string(),
            cwd: task.workspace_root.clone(),
            model_id: task.model_id.clone(),
            cancellation: TurnCancellation::new(),
        }) {
            Ok(_) => Ok(true),
            Err(RuntimeError::CapabilityMissing(capability))
                if capability == "acp_session_resume_after_runtime_restart" =>
            {
                Ok(false)
            }
            Err(error) => Err(protocol_error_from_runtime(error)),
        }
    }

    fn native_session_for_task(
        &self,
        task: &TaskRecord,
        session_id: &str,
    ) -> Result<Option<AgentListedSession>, ProtocolError> {
        let sessions =
            self.history_sync
                .listed_sessions(&task.agent_id, &task.workspace_root, || {
                    self.fetch_native_sessions(task)
                })?;
        Ok(sessions
            .into_iter()
            .find(|session| session.session_id == session_id))
    }

    fn fetch_native_sessions(
        &self,
        task: &TaskRecord,
    ) -> Result<Vec<AgentListedSession>, ProtocolError> {
        let mut cursor = None;
        let mut sessions = Vec::new();
        loop {
            let result = match self.agent_gateway.list_sessions(AgentListSessionsRequest {
                agent_id: task.agent_id.clone(),
                cwd: task.workspace_root.clone(),
                cursor: cursor.clone(),
            }) {
                Ok(result) => result,
                Err(error) => return Err(protocol_error_from_runtime(error)),
            };
            sessions.extend(result.sessions);
            if result.next_cursor.is_none() || result.next_cursor == cursor {
                return Ok(sessions);
            }
            cursor = result.next_cursor;
        }
    }
}

impl TaskOpenWorkflow for TaskProductApi {
    fn open(&self, params: TaskOpenParams) -> Result<TaskSnapshot, ProtocolError> {
        self.open_task(params)
    }

    fn mark_read(&self, params: TaskMarkReadParams) -> Result<TaskSnapshot, ProtocolError> {
        self.mark_task_read(params)
    }

    fn retry_history_sync(
        &self,
        params: TaskRetryHistorySyncParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let Some(pending) = self.history_sync.take_deferred_send(&task_id) else {
            return self.open_task(TaskOpenParams {
                task_id: params.task_id,
            });
        };
        let task = self.store.read_task(&task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&task)?;
        let snapshot = crate::tasks::snapshot::build_snapshot(&self.store, &task_id, 100)
            .map_err(super::storage_error)?;
        let sync_generation = self.history_sync.begin_send(&task_id);
        self.publish_history_sync(
            &task_id,
            openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Syncing {
                generation: sync_generation,
            },
        );
        let api = self.clone();
        std::thread::spawn(move || {
            if let Err(error) = api.start_committed_send(
                task,
                pending.prompt_text,
                pending.attachments,
                pending.committed_send,
                sync_generation,
            ) {
                logging::error(
                    "task_history_sync_retry_failed",
                    serde_json::json!({ "task_id": task_id, "error": error.message }),
                );
            }
        });
        project_stored_task_snapshot(snapshot)
    }
}
