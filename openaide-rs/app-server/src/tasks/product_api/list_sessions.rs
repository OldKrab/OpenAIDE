use openaide_app_server_protocol::agent::{
    AgentListSessionsParams, AgentListSessionsResult, AgentListedSession,
};
use openaide_app_server_protocol::errors::ProtocolError;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::agent::{AgentListSessionsRequest, AgentSessionKey};
use crate::native_sessions::catalog::{NativeSessionObservation, NativeSessionRef};
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};

use super::session_cursor::OpaqueSessionCursor;
use super::{protocol_error_from_runtime, AgentListSessionsWorkflow, TaskProductApi};

#[derive(Clone, Default)]
pub(super) struct NativeCatalogRefreshCoordinator {
    state: Arc<Mutex<NativeCatalogRefreshState>>,
}

#[derive(Default)]
struct NativeCatalogRefreshState {
    running: bool,
    trailing_run_requested: bool,
}

impl TaskProductApi {
    pub(crate) fn request_native_session_catalog_load_more(
        &self,
        project_id: &str,
        target_row_count: usize,
    ) {
        self.request_native_session_catalog_refresh_for_project_target(
            openaide_app_server_protocol::ids::ProjectId::from(project_id.to_string()),
            target_row_count,
        );
    }

    /// Coalesces catalog work while preserving one trailing refresh requested during a run.
    pub(crate) fn request_native_session_catalog_refresh(&self) {
        {
            let mut state = self
                .native_catalog_refresh
                .state
                .lock()
                .expect("Native Session catalog refresh state poisoned");
            if state.running {
                state.trailing_run_requested = true;
                return;
            }
            state.running = true;
        }

        self.native_catalog.set_refreshing(true);
        self.task_notifier.navigation_refresh_state_changed(
            openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Refreshing,
        );

        let api = self.clone();
        std::thread::spawn(move || loop {
            let started_at = Instant::now();
            crate::logging::info(
                "native_session_catalog_refresh_started",
                serde_json::json!({ "operation": "agent/list_sessions" }),
            );
            let refresh = api.refresh_native_session_catalogs();
            let mut state = api
                .native_catalog_refresh
                .state
                .lock()
                .expect("Native Session catalog refresh state poisoned");
            if state.trailing_run_requested {
                state.trailing_run_requested = false;
                continue;
            }
            state.running = false;
            let refresh = match refresh {
                Ok(()) => {
                    crate::logging::info(
                        "native_session_catalog_refresh_completed",
                        serde_json::json!({
                            "operation": "agent/list_sessions",
                            "duration_ms": started_at.elapsed().as_millis(),
                        }),
                    );
                    openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Idle
                }
                Err(error) => {
                    crate::logging::warn(
                        "native_session_catalog_refresh_failed",
                        serde_json::json!({
                            "operation": "agent/list_sessions",
                            "duration_ms": started_at.elapsed().as_millis(),
                            "error": error.message,
                        }),
                    );
                    openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Failed {
                        message: error.message,
                    }
                }
            };
            api.native_catalog.set_refresh_state(refresh.clone());
            api.task_notifier.navigation_refresh_state_changed(refresh);
            break;
        });
    }

    fn request_native_session_catalog_refresh_for_project_target(
        &self,
        project_id: openaide_app_server_protocol::ids::ProjectId,
        target_row_count: usize,
    ) {
        if self
            .native_catalog
            .set_project_refreshing(project_id.as_str(), true)
        {
            self.task_notifier
                .navigation_project_entries_changed(project_id.as_str().to_string());
        }
        let api = self.clone();
        std::thread::spawn(move || {
            if let Err(error) = api.refresh_native_session_project_trees(
                Some(project_id.as_str()),
                Some(target_row_count),
            ) {
                crate::logging::warn(
                    "native_session_project_catalog_refresh_failed",
                    serde_json::json!({
                        "project_id": project_id.as_str(),
                        "error": error.message,
                    }),
                );
            }
            api.native_catalog
                .set_project_refreshing(project_id.as_str(), false);
            api.task_notifier
                .navigation_project_entries_changed(project_id.as_str().to_string());
        });
    }

    pub(super) fn refresh_native_session_catalogs(&self) -> Result<(), ProtocolError> {
        self.refresh_native_session_catalogs_for(None)
    }

    fn refresh_native_session_catalogs_for(
        &self,
        project_filter: Option<&str>,
    ) -> Result<(), ProtocolError> {
        self.refresh_native_session_project_trees(project_filter, None)
    }

    /// Advances owned Task activity and records possible external changes without replacing
    /// an active attachment. Catalog timestamps cannot distinguish history from option/title
    /// changes, so replay remains an explicit Task action.
    pub(super) fn reconcile_native_session_activity(
        &self,
        agent_id: &str,
        workspace_root: &str,
        sessions: &[crate::protocol::model::AgentListedSession],
        task_records: &[TaskRecord],
    ) -> Result<(), ProtocolError> {
        let metadata = sessions
            .iter()
            .map(|session| (&session.session_id, session))
            .collect::<std::collections::HashMap<_, _>>();
        for record in task_records.iter().filter(|task| {
            !task.tombstoned
                && task.agent_id == agent_id
                && task.workspace_root == workspace_root
                && task
                    .agent_session_id
                    .as_ref()
                    .is_some_and(|session_id| metadata.contains_key(session_id))
        }) {
            let expected_session_id = record
                .agent_session_id
                .clone()
                .expect("matched Task has a Native Session");
            let session = metadata[&expected_session_id];
            let native_activity = [
                session.last_activity.as_deref(),
                session.updated_at.as_deref(),
            ]
            .into_iter()
            .flatten()
            .filter_map(|value| crate::time::activity_millis(value).map(|time| (time, value)))
            .max_by_key(|(time, _)| *time)
            .map(|(_, value)| value.to_string());
            let deferred_reload_activity = native_activity
                .as_ref()
                .filter(|activity| {
                    let Some(native_time) = crate::time::activity_millis(activity) else {
                        return false;
                    };
                    let Ok(local_history_updated_at) =
                        self.store.local_history_updated_at(&record.task_id)
                    else {
                        return false;
                    };
                    crate::time::activity_millis(&local_history_updated_at)
                        .is_some_and(|local_time| native_time > local_time.saturating_add(5_000))
                })
                .cloned();
            let reload_requirement_changed = std::cell::Cell::new(false);
            self.mutations
                .commit_existing_task(&record.task_id, TaskCommitOptions::metadata(), |ctx| {
                    let task = ctx.task_mut();
                    if task.tombstoned
                        || task.agent_id != agent_id
                        || task.workspace_root != workspace_root
                        || task.agent_session_id.as_deref() != Some(expected_session_id.as_str())
                    {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    let mut changed = false;
                    if let Some(native_activity) = &native_activity {
                        let native_time = crate::time::activity_millis(native_activity);
                        let task_time = crate::time::activity_millis(&task.last_activity);
                        if native_time
                            .zip(task_time)
                            .is_some_and(|(native, task)| native > task)
                        {
                            task.last_activity = native_activity.clone();
                            changed = true;
                        }
                    }
                    if matches!(task.lifecycle, TaskLifecycle::Open)
                        && matches!(task.status, crate::protocol::model::TaskStatus::Inactive)
                        && task.active_turn_id.is_none()
                        && deferred_reload_activity.as_ref().is_some_and(|activity| {
                            task.mark_native_session_reload_required(activity.clone())
                        })
                    {
                        reload_requirement_changed.set(true);
                        changed = true;
                    }
                    Ok(if changed {
                        TaskMutationResult::Changed
                    } else {
                        TaskMutationResult::Unchanged
                    })
                })
                .map_err(protocol_error_from_runtime)?;
            if reload_requirement_changed.get() {
                self.publish_history_sync(
                    &record.task_id,
                    self.history_sync.reload_available_snapshot(&record.task_id),
                );
            }
        }
        Ok(())
    }

    fn list_sessions_for_project(
        &self,
        params: AgentListSessionsParams,
    ) -> Result<AgentListSessionsResult, ProtocolError> {
        let project = self
            .project_resolver
            .resolve_task_context(&params.project_id)?;
        self.agent_registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        let agent_id = params.agent_id.clone();
        let mut cursor = OpaqueSessionCursor::new(params.cursor);
        loop {
            let result = self
                .agent_gateway
                .list_sessions(AgentListSessionsRequest {
                    agent_id: params.agent_id.as_str().to_string(),
                    cwd: Some(project.workspace_root.clone()),
                    cursor: cursor.current(),
                })
                .map_err(protocol_error_from_runtime)?;
            let next_cursor = cursor.advance(result.next_cursor);
            let task_records = self
                .store
                .list_all_task_records_strict()
                .map_err(protocol_error_from_runtime)?;
            self.reconcile_native_session_activity(
                params.agent_id.as_str(),
                &project.workspace_root,
                &result.sessions,
                &task_records,
            )?;
            self.record_native_catalog_page(
                project.project_id.as_str(),
                params.agent_id.as_str(),
                &project.workspace_root,
                &result.sessions,
            )?;
            let sessions = self
                .unowned_native_sessions(params.agent_id.as_str(), result.sessions, &task_records)?
                .into_iter()
                .map(|session| AgentListedSession {
                    session_id: session.session_id,
                    title: session.title,
                    last_activity: session.last_activity,
                    updated_at: session.updated_at,
                })
                .collect::<Vec<_>>();
            if !sessions.is_empty() || next_cursor.is_none() {
                return Ok(AgentListSessionsResult {
                    agent_id,
                    project_id: project.project_id,
                    project_label: project.label,
                    sessions,
                    next_cursor,
                });
            }
        }
    }

    pub(super) fn record_native_catalog_page(
        &self,
        project_id: &str,
        agent_id: &str,
        workspace_root: &str,
        sessions: &[crate::protocol::model::AgentListedSession],
    ) -> Result<(), ProtocolError> {
        self.native_catalog
            .record_page(
                project_id,
                workspace_root,
                sessions
                    .iter()
                    .map(|session| NativeSessionObservation {
                        reference: NativeSessionRef::new(agent_id, &session.session_id),
                        title: session.title.clone(),
                        last_activity: session
                            .last_activity
                            .clone()
                            .or_else(|| session.updated_at.clone()),
                    })
                    .collect(),
            )
            .map_err(protocol_error_from_runtime)?;
        self.task_notifier
            .navigation_project_entries_changed(project_id.to_string());
        Ok(())
    }

    fn unowned_native_sessions(
        &self,
        agent_id: &str,
        sessions: Vec<crate::protocol::model::AgentListedSession>,
        records: &[TaskRecord],
    ) -> Result<Vec<crate::protocol::model::AgentListedSession>, ProtocolError> {
        let mut owned: std::collections::HashSet<AgentSessionKey> = records
            .iter()
            .filter(|record| record.agent_id == agent_id)
            .filter_map(|record| {
                record.agent_session_id.as_ref().map(|session_id| {
                    AgentSessionKey::new(record.agent_id.clone(), session_id.clone())
                })
            })
            .collect();
        owned.extend(
            self.preparing_session_ids
                .lock()
                .map_err(|_| {
                    protocol_error_from_runtime(crate::protocol::errors::RuntimeError::Internal(
                        "preparing session ownership lock poisoned".to_string(),
                    ))
                })?
                .iter()
                .cloned(),
        );
        Ok(sessions
            .into_iter()
            .filter(|session| {
                !owned.contains(&AgentSessionKey::new(agent_id, session.session_id.clone()))
            })
            .collect())
    }
}

impl AgentListSessionsWorkflow for TaskProductApi {
    fn list_agent_sessions(
        &self,
        params: AgentListSessionsParams,
    ) -> Result<AgentListSessionsResult, ProtocolError> {
        self.list_sessions_for_project(params)
    }

    fn request_native_session_catalog_refresh(&self) {
        TaskProductApi::request_native_session_catalog_refresh(self)
    }

    fn request_native_session_catalog_load_more(&self, project_id: &str, target_row_count: usize) {
        TaskProductApi::request_native_session_catalog_load_more(self, project_id, target_row_count)
    }
}
