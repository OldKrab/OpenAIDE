use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::{TaskMarkReadParams, TaskOpenParams};

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

const NATIVE_HISTORY_CLOCK_TOLERANCE_MS: i128 = 5_000;

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
            .ok_or_else(|| internal_error("missing task open snapshot"))?;
        self.spawn_adopted_task_refresh(task);
        self.project_task_snapshot(snapshot)
    }

    /// Task opening returns cached state first, then recovers the Native Session without
    /// synchronously listing Agent sessions. The cached clock selects resume or history replay.
    fn spawn_adopted_task_refresh(&self, task: TaskRecord) {
        if matches!(task.lifecycle, TaskLifecycle::New { .. })
            || matches!(
                task.status,
                LegacyTaskStatus::Starting | LegacyTaskStatus::Active
            )
            || task.active_turn_id.is_some()
        {
            return;
        }
        let Some(stored_session_id) = task.agent_session_id.clone() else {
            return;
        };
        let native_session = self.history_sync.cached_session(
            &task.agent_id,
            &task.workspace_root,
            &stored_session_id,
        );
        if native_session.is_none() {
            self.spawn_adopted_task_resume(task, stored_session_id, None);
            return;
        }
        let native_session = native_session.expect("checked above");
        let Ok(local_history_updated_at) = self.store.local_history_updated_at(&task.task_id)
        else {
            self.spawn_adopted_task_resume(task, stored_session_id, Some(native_session));
            return;
        };
        let Some((native_updated_at, refreshed_at)) =
            newer_native_activity(&native_session, &local_history_updated_at)
        else {
            self.spawn_adopted_task_resume(task, stored_session_id, Some(native_session));
            return;
        };
        let Some(generation) = self.history_sync.begin_passive(&task.task_id) else {
            return;
        };
        let api = self.clone();
        std::thread::spawn(move || {
            let result = api.history_sync.run_passive(&task.task_id, generation, || {
                api.publish_history_sync(
                    &task.task_id,
                    openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Syncing {
                        generation: generation.value(),
                    },
                );
                let Ok(local_history_updated_at) =
                    api.store.local_history_updated_at(&task.task_id)
                else {
                    return Ok(None);
                };
                if newer_native_activity(&native_session, &local_history_updated_at).is_none() {
                    return Ok(None);
                }
                api.native_sessions
                    .refresh_history(HistoryRefreshRequest {
                        task: task.clone(),
                        stored_session_id,
                        native_session,
                        native_updated_at,
                        refreshed_at,
                    })
                    .map_err(protocol_error_from_runtime)
            });
            if !api.history_sync.is_current(&task.task_id, generation) {
                return;
            }
            match result.unwrap_or(Ok(None)) {
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
                    api.publish_history_sync(
                        &task.task_id,
                        openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle {
                            generation: generation.value(),
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

    /// Session recovery is independent from history replay. A fresh or unordered
    /// Chat still needs its Agent connection and live catalogs restored.
    fn spawn_adopted_task_resume(
        &self,
        task: TaskRecord,
        stored_session_id: String,
        native_session: Option<AgentListedSession>,
    ) {
        let Some(generation) = self.history_sync.begin_passive(&task.task_id) else {
            return;
        };
        let api = self.clone();
        std::thread::spawn(move || {
            let load_started = std::cell::Cell::new(false);
            let result = api.history_sync.run_passive(&task.task_id, generation, || {
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
                            })
                            .map_err(protocol_error_from_runtime)
                    }
                }
            });
            if !api.history_sync.is_current(&task.task_id, generation) {
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
                    api.publish_history_sync(
                        &task.task_id,
                        openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle {
                            generation: generation.value(),
                        },
                    );
                }
                (false, Some(Err(error))) => logging::warn(
                    "adopted_task_background_resume_failed",
                    serde_json::json!({
                        "task_id": task.task_id,
                        "agent_id": task.agent_id,
                        "error": error.message,
                    }),
                ),
                (false, _) => {}
            }
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
}

/// Native history replacement is destructive, so missing or incomparable clocks never win.
fn newer_native_activity(
    native_session: &AgentListedSession,
    local_history_updated_at: &str,
) -> Option<(u128, String)> {
    let (native_time, native_value) = [
        native_session.last_activity.as_deref(),
        native_session.updated_at.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|value| crate::time::activity_millis(value).map(|time| (time, value)))
    .max_by_key(|(time, _)| *time)?;
    let local_time = crate::time::activity_millis(local_history_updated_at)?;
    (native_time > local_time.saturating_add(NATIVE_HISTORY_CLOCK_TOLERANCE_MS))
        .then(|| {
            u128::try_from(native_time)
                .ok()
                .map(|native_time| (native_time, native_value.to_string()))
        })
        .flatten()
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
}
