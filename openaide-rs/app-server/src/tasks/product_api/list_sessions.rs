use openaide_app_server_protocol::agent::{
    AgentListSessionsParams, AgentListSessionsResult, AgentListedSession,
};
use openaide_app_server_protocol::errors::ProtocolError;
use std::sync::{Arc, Mutex};

use crate::agent::{AgentListSessionsRequest, AgentSessionKey};
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

        let api = self.clone();
        std::thread::spawn(move || loop {
            if let Err(error) = api.refresh_native_session_catalogs() {
                crate::logging::warn(
                    "native_session_catalog_refresh_failed",
                    serde_json::json!({ "error": error.message }),
                );
            }
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
            break;
        });
    }

    pub(super) fn refresh_native_session_catalogs(&self) -> Result<(), ProtocolError> {
        let task_records = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?;
        let contexts = task_records
            .iter()
            .filter(|task| !task.tombstoned && task.agent_session_id.is_some())
            .map(|task| (task.agent_id.clone(), task.workspace_root.clone()))
            .collect::<std::collections::HashSet<_>>();
        let mut first_error = None;
        for (agent_id, workspace_root) in contexts {
            match self.fetch_complete_native_catalog(&agent_id, &workspace_root) {
                Ok(sessions) => {
                    self.reconcile_native_session_titles(
                        &agent_id,
                        &workspace_root,
                        &sessions,
                        &task_records,
                    )?;
                    self.history_sync
                        .replace_listed_sessions(&agent_id, &workspace_root, sessions)
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Reconciles Agent-owned title metadata without coupling it to Chat history refresh.
    fn reconcile_native_session_titles(
        &self,
        agent_id: &str,
        workspace_root: &str,
        sessions: &[crate::protocol::model::AgentListedSession],
        task_records: &[TaskRecord],
    ) -> Result<(), ProtocolError> {
        let titles = sessions
            .iter()
            .filter_map(|session| {
                session
                    .title
                    .as_deref()
                    .map(|title| (&session.session_id, title))
            })
            .collect::<std::collections::HashMap<_, _>>();
        for record in task_records.iter().filter(|task| {
            !task.tombstoned
                && task.agent_id == agent_id
                && task.workspace_root == workspace_root
                && task
                    .agent_session_id
                    .as_ref()
                    .is_some_and(|session_id| titles.contains_key(session_id))
        }) {
            let expected_session_id = record
                .agent_session_id
                .clone()
                .expect("matched Task has a Native Session");
            let title = titles[&expected_session_id].to_string();
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
                    Ok(if task.set_agent_title(&title) {
                        TaskMutationResult::Changed
                    } else {
                        TaskMutationResult::Unchanged
                    })
                })
                .map_err(protocol_error_from_runtime)?;
        }
        Ok(())
    }

    fn fetch_complete_native_catalog(
        &self,
        agent_id: &str,
        workspace_root: &str,
    ) -> Result<Vec<crate::protocol::model::AgentListedSession>, ProtocolError> {
        let mut cursor = OpaqueSessionCursor::new(None);
        let mut sessions = Vec::new();
        loop {
            let result = self
                .agent_gateway
                .list_sessions(AgentListSessionsRequest {
                    agent_id: agent_id.to_string(),
                    cwd: workspace_root.to_string(),
                    cursor: cursor.current(),
                })
                .map_err(protocol_error_from_runtime)?;
            sessions.extend(result.sessions);
            if cursor.advance(result.next_cursor).is_none() {
                return Ok(sessions);
            }
        }
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
                    cwd: project.workspace_root.clone(),
                    cursor: cursor.current(),
                })
                .map_err(protocol_error_from_runtime)?;
            let next_cursor = cursor.advance(result.next_cursor);
            let task_records = self
                .store
                .list_all_task_records_strict()
                .map_err(protocol_error_from_runtime)?;
            self.reconcile_native_session_titles(
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
}
