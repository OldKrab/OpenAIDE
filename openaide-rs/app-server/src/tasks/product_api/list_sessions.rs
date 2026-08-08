use openaide_app_server_protocol::agent::{
    AgentListSessionsParams, AgentListSessionsResult, AgentListedSession,
};
use openaide_app_server_protocol::errors::ProtocolError;
use std::sync::{Arc, Mutex};

use crate::agent::{AgentListSessionsRequest, AgentSessionKey};
use crate::native_sessions::catalog::{NativeSessionObservation, NativeSessionRef};
use crate::projects::ProjectIdentity;
use crate::storage::records::TaskRecord;
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
    /// Refreshes the catalog entry that decides how an existing Task reconnects.
    ///
    /// Task opening remains non-blocking; this targeted read runs in its recovery
    /// worker so cached Chat can render before one authoritative resume/load choice.
    pub(super) fn refresh_bound_session_for_open(
        &self,
        task: &TaskRecord,
        stored_session_id: &str,
    ) -> Result<Option<crate::protocol::model::AgentListedSession>, ProtocolError> {
        self.agent_registry
            .require(&task.agent_id)
            .map_err(protocol_error_from_runtime)?;
        let project_root = task.project_root.as_deref().unwrap_or(&task.workspace_root);
        let project_id = ProjectIdentity::from_workspace_root(project_root).project_id;
        let mut cursor = OpaqueSessionCursor::new(None);
        loop {
            let result = self
                .agent_gateway
                .list_sessions(AgentListSessionsRequest {
                    agent_id: task.agent_id.clone(),
                    cwd: Some(task.workspace_root.clone()),
                    cursor: cursor.current(),
                })
                .map_err(protocol_error_from_runtime)?;
            let matching = result
                .sessions
                .iter()
                .find(|session| session.session_id == stored_session_id)
                .cloned();
            self.reconcile_native_session_activity(
                &task.agent_id,
                &task.workspace_root,
                &result.sessions,
                std::slice::from_ref(task),
            )?;
            self.history_sync.record_listed_sessions(
                &task.agent_id,
                &task.workspace_root,
                &result.sessions,
            );
            self.record_native_catalog_page(
                project_id.as_str(),
                &task.agent_id,
                &task.workspace_root,
                &result.sessions,
            )?;
            if matching.is_some() {
                return Ok(matching);
            }
            if cursor.advance(result.next_cursor).is_none() {
                return Ok(None);
            }
        }
    }

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
                Ok(()) => openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Idle,
                Err(error) => {
                    crate::logging::warn(
                        "native_session_catalog_refresh_failed",
                        serde_json::json!({ "error": error.message }),
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

    pub(super) fn refresh_subscribed_task_histories(
        &self,
        agent_id: &str,
        workspace_root: &str,
        task_records: &[TaskRecord],
    ) {
        for task in task_records.iter().filter(|task| {
            task.agent_id == agent_id
                && task.workspace_root == workspace_root
                && task.agent_session_id.is_some()
        }) {
            self.spawn_subscribed_task_history_refresh(task.clone());
        }
    }

    /// Advances owned Task activity from listings without importing stale runtime metadata.
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
                    Ok(if changed {
                        TaskMutationResult::Changed
                    } else {
                        TaskMutationResult::Unchanged
                    })
                })
                .map_err(protocol_error_from_runtime)?;
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
            self.history_sync.record_listed_sessions(
                params.agent_id.as_str(),
                &project.workspace_root,
                &result.sessions,
            );
            self.refresh_subscribed_task_histories(
                params.agent_id.as_str(),
                &project.workspace_root,
                &task_records,
            );
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
