use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::{
    TaskMarkReadParams, TaskOpenParams, TaskReloadNativeSessionParams,
};

use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, AgentListedSession, NormalizedMessage,
    TaskStatus as LegacyTaskStatus,
};
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::tasks::mutation::TaskMutationResult;
use crate::tasks::native_session_service::HistoryRefreshRequest;
use crate::tasks::native_session_service::OpenSessionResumeOutcome;

use super::{internal_error, protocol_error_from_runtime, TaskProductApi};

pub(crate) trait TaskOpenWorkflow: Send + Sync {
    fn open_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskOpenParams,
    ) -> Result<TaskSnapshot, ProtocolError>;
    fn mark_read_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskMarkReadParams,
    ) -> Result<TaskSnapshot, ProtocolError>;
    fn reload_native_session_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: TaskReloadNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        Err(ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::CapabilityUnavailable,
            message: "Task Native Session reload is unavailable".to_string(),
            recoverable: false,
            target: None,
        })
    }
}

impl TaskProductApi {
    pub(super) fn mark_task_read(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskMarkReadParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.read_task_for_client(&task_id, client_instance_id)?;
        let result = self
            .mutations
            .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                if ctx.task().tombstoned {
                    return Err(RuntimeError::TaskNotFound(task_id.clone()));
                }
                if !ctx.task().unread && ctx.task().attention.is_none() {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                task.unread = false;
                task.attention = None;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing task mark-read snapshot"))?;
        self.project_task_snapshot(snapshot)
    }

    pub(super) fn open_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskOpenParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let task = self.read_task_for_client(&task_id, client_instance_id)?;
        let recovering_native_session =
            task_allows_history_recovery(&task) && task.agent_session_id.is_some();
        self.mark_task_used_now(&task_id)
            .map_err(protocol_error_from_runtime)?;

        if matches!(task.lifecycle, TaskLifecycle::Archived) {
            let snapshot = crate::tasks::snapshot::build_snapshot(
                &self.store,
                &task_id,
                crate::chat_history::ChatHistoryPolicy::default().task_snapshot_tail_limit(),
            )
            .map_err(super::protocol_error_from_runtime)?;
            return self.project_task_snapshot(snapshot);
        }

        let result = self
            .mutations
            .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                if ctx.task().tombstoned {
                    return Err(RuntimeError::TaskNotFound(task_id.clone()));
                }
                let mut changed = false;
                if ctx.task().unread || ctx.task().attention.is_some() {
                    let task = ctx.task_mut();
                    task.unread = false;
                    task.attention = None;
                    changed = true;
                }
                if recovering_native_session {
                    changed |= ctx.task_mut().mark_native_session_data_recovering();
                }
                Ok(if changed {
                    TaskMutationResult::Changed
                } else {
                    TaskMutationResult::Unchanged
                })
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing task open snapshot"))?;
        self.spawn_adopted_task_refresh(task);
        self.project_task_snapshot(snapshot)
    }

    /// Task opening returns cached state first, then restores the known binding directly.
    /// Catalog observation is discovery work and must never gate `session/resume`.
    fn spawn_adopted_task_refresh(&self, task: TaskRecord) {
        if !task_allows_history_recovery(&task) {
            return;
        }
        let Some(stored_session_id) = task.agent_session_id.clone() else {
            return;
        };
        let Some(generation) = self.history_sync.begin_passive(&task.task_id) else {
            return;
        };
        if let Some(requirement) = task.native_session_reload_requirement.clone() {
            self.spawn_adopted_task_load(
                task,
                stored_session_id,
                requirement.observed_activity_at,
                generation,
            );
        } else {
            self.spawn_adopted_task_resume(task, stored_session_id, None, generation);
        }
    }

    /// A deferred requirement belongs to the next attachment recovery, so open loads first
    /// instead of resuming and later overwriting controls with a second restore.
    fn spawn_adopted_task_load(
        &self,
        task: TaskRecord,
        stored_session_id: String,
        observed_activity_at: String,
        generation: crate::tasks::history_sync::PassiveSyncGeneration,
    ) {
        let api = self.clone();
        std::thread::spawn(move || {
            let native_updated_at = crate::time::activity_millis(&observed_activity_at)
                .and_then(|time| u128::try_from(time).ok());
            let result = native_updated_at.map(|native_updated_at| {
                api.history_sync
                    .run_passive(&task.task_id, &generation, || {
                        api.publish_history_sync(
                        &task.task_id,
                        openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Syncing {
                            generation: generation.value(),
                        },
                    );
                        api.native_sessions
                            .refresh_history(HistoryRefreshRequest {
                                task: task.clone(),
                                stored_session_id: stored_session_id.clone(),
                                native_session: AgentListedSession {
                                    session_id: stored_session_id.clone(),
                                    cwd: task.workspace_root.clone(),
                                    title: None,
                                    last_activity: Some(observed_activity_at.clone()),
                                    updated_at: Some(observed_activity_at.clone()),
                                },
                                native_updated_at,
                                refreshed_at: crate::time::now_string(),
                                clear_reload_requirement_through: Some(
                                    observed_activity_at.clone(),
                                ),
                            })
                            .map_err(protocol_error_from_runtime)
                    })
            });
            if !api.history_sync.is_current(&task.task_id, &generation) {
                return;
            }
            match result.flatten() {
                Some(Ok(Some(_))) => api.publish_history_sync(
                    &task.task_id,
                    openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Updated {
                        generation: generation.value(),
                    },
                ),
                Some(Ok(None)) | None => api.publish_history_sync(
                    &task.task_id,
                    api.history_sync.reload_available_snapshot(&task.task_id),
                ),
                Some(Err(error)) => {
                    api.mark_native_session_recovery_stale(&task.task_id, &stored_session_id);
                    api.publish_history_sync(
                        &task.task_id,
                        api.history_sync.reload_available_snapshot(&task.task_id),
                    );
                    logging::warn(
                        "native_session_open_load_failed",
                        serde_json::json!({
                            "task_id": task.task_id,
                            "agent_id": task.agent_id,
                            "error_code": format!("{:?}", error.code),
                        }),
                    );
                }
            }
            api.history_sync.finish_passive(&task.task_id, &generation);
        });
    }

    /// Session recovery is independent from history replay. A fresh or unordered
    /// Chat still needs its Agent connection and live catalogs restored.
    fn spawn_adopted_task_resume(
        &self,
        task: TaskRecord,
        stored_session_id: String,
        native_session: Option<AgentListedSession>,
        generation: crate::tasks::history_sync::PassiveSyncGeneration,
    ) {
        let api = self.clone();
        std::thread::spawn(move || {
            let load_started = std::cell::Cell::new(false);
            let result = api.history_sync.run_passive(&task.task_id, &generation, || {
                match api
                    .native_sessions
                    .resume_for_open(&task, &stored_session_id)
                    .map_err(protocol_error_from_runtime)?
                {
                    OpenSessionResumeOutcome::Resumed => Ok(None),
                    OpenSessionResumeOutcome::Unsupported => {
                        load_started.set(true);
                        api.publish_history_sync(
                            &task.task_id,
                            openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Syncing {
                                generation: generation.value(),
                            },
                        );
                        let refreshed_at = crate::time::now_string();
                        let native_updated_at = refreshed_at.parse::<u128>().map_err(|_| {
                            protocol_error_from_runtime(RuntimeError::Internal(
                                "current history recovery timestamp is invalid".to_string(),
                            ))
                        })?;
                        let native_session = native_session.unwrap_or_else(|| AgentListedSession {
                            session_id: stored_session_id.clone(),
                            cwd: task.workspace_root.clone(),
                            title: None,
                            last_activity: None,
                            updated_at: None,
                        });
                        api.native_sessions
                            .refresh_history(HistoryRefreshRequest {
                                task: task.clone(),
                                stored_session_id: stored_session_id.clone(),
                                native_session,
                                native_updated_at,
                                refreshed_at,
                                clear_reload_requirement_through: None,
                            })
                            .map_err(protocol_error_from_runtime)
                    }
                }
            });
            if !api.history_sync.is_current(&task.task_id, &generation) {
                return;
            }
            match (load_started.get(), result) {
                (true, Some(Ok(Some(_)))) => api.publish_history_sync(
                    &task.task_id,
                    openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Updated {
                        generation: generation.value(),
                    },
                ),
                (true, Some(Ok(None))) | (true, None) => api.publish_history_sync(
                    &task.task_id,
                    openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle {
                        generation: generation.value(),
                    },
                ),
                (true, Some(Err(error))) => {
                    if let Err(persist_error) =
                        api.record_history_update_failure(&task.task_id, &error.message)
                    {
                        logging::warn(
                            "history_update_failure_activity_persist_failed",
                            serde_json::json!({
                                "task_id": task.task_id,
                                "error": persist_error.to_string(),
                            }),
                        );
                    }
                    api.mark_native_session_recovery_stale(&task.task_id, &stored_session_id);
                    api.publish_history_sync(
                        &task.task_id,
                        openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle {
                            generation: generation.value(),
                        },
                    );
                }
                (false, Some(Err(error))) => {
                    api.mark_native_session_recovery_stale(&task.task_id, &stored_session_id);
                    logging::warn(
                        "adopted_task_background_resume_failed",
                        serde_json::json!({
                            "task_id": task.task_id,
                            "agent_id": task.agent_id,
                            "error": error.message,
                        }),
                    );
                }
                (false, _) => {}
            }
            api.history_sync.finish_passive(&task.task_id, &generation);
        });
    }

    fn record_history_update_failure(
        &self,
        task_id: &str,
        message: &str,
    ) -> Result<(), RuntimeError> {
        let now = crate::time::now_string();
        let activity_id = format!("history-sync-failure:{}", uuid::Uuid::new_v4());
        self.mutations.commit_existing_task(
            task_id,
            super::response_snapshot_options(),
            |ctx| {
                ctx.append_message(NormalizedMessage::Activity {
                    id: activity_id,
                    title: "History update failed".to_string(),
                    status: ActivityStatus::Error,
                    created_at: now.clone(),
                    collapsed: true,
                    steps: vec![ActivityStep::Text {
                        text: message.to_string(),
                        level: Some("error".to_string()),
                    }],
                })?;
                ctx.task_mut().updated_at = now.clone();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    /// Ends the open-time loading state only if this is still the failed attachment attempt.
    pub(super) fn mark_native_session_recovery_stale(&self, task_id: &str, session_id: &str) {
        if let Err(error) = self.mutations.commit_existing_task(
            task_id,
            super::response_snapshot_options(),
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(session_id)
                    || !ctx.task().native_session_data_freshness.is_recovering()
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.task_mut().mark_native_session_data_stale();
                Ok(TaskMutationResult::Changed)
            },
        ) {
            logging::warn(
                "adopted_task_recovery_state_persist_failed",
                serde_json::json!({
                    "task_id": task_id,
                    "error_code": error.code(),
                    "error_kind": error.reason(),
                }),
            );
        }
    }
}

fn task_allows_history_recovery(task: &TaskRecord) -> bool {
    !matches!(
        task.lifecycle,
        TaskLifecycle::Prepared { .. } | TaskLifecycle::Archived
    ) && !matches!(
        task.status,
        LegacyTaskStatus::Starting | LegacyTaskStatus::Active
    ) && task.active_turn_id.is_none()
}

impl TaskOpenWorkflow for TaskProductApi {
    fn open_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskOpenParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.open_task(client_instance_id, params)
    }

    fn mark_read_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskMarkReadParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.mark_task_read(client_instance_id, params)
    }

    fn reload_native_session_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskReloadNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.reload_native_session(client_instance_id, params)
    }
}
