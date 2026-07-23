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
    project_targets: std::collections::HashMap<String, usize>,
}

#[derive(Default)]
struct ListingOrderValidator {
    trusted: bool,
    previous_page_frontier: Option<i128>,
}

struct PageOrderObservation {
    activity_frontier: Option<i128>,
    new_identity_count: usize,
}

impl ListingOrderValidator {
    fn new() -> Self {
        Self {
            trusted: true,
            previous_page_frontier: None,
        }
    }

    /// Validates Agent order before catalog normalization can hide malformed timestamps.
    fn observe(
        &mut self,
        sessions: &[crate::protocol::model::AgentListedSession],
        seen: &mut std::collections::HashSet<String>,
    ) -> PageOrderObservation {
        let mut page_times = Vec::new();
        let mut previous_time = None;
        let mut new_identity_count = 0;
        for session in sessions {
            if !seen.insert(session.session_id.clone()) {
                continue;
            }
            new_identity_count += 1;
            let time = listed_session_activity(session);
            let Some(time) = time else {
                self.trusted = false;
                continue;
            };
            if previous_time.is_some_and(|previous| time > previous) {
                self.trusted = false;
            }
            previous_time = Some(time);
            page_times.push(time);
        }
        if let (Some(previous_frontier), Some(page_max)) = (
            self.previous_page_frontier,
            page_times.iter().copied().max(),
        ) {
            if page_max > previous_frontier {
                self.trusted = false;
            }
        }
        let activity_frontier = page_times.iter().copied().min();
        if activity_frontier.is_some() {
            self.previous_page_frontier = activity_frontier;
        }
        PageOrderObservation {
            activity_frontier,
            new_identity_count,
        }
    }
}

impl TaskProductApi {
    pub(crate) fn request_native_session_catalog_load_more(
        &self,
        project_id: &str,
        target_row_count: usize,
    ) {
        self.native_catalog_refresh
            .state
            .lock()
            .expect("Native Session catalog refresh state poisoned")
            .project_targets
            .entry(project_id.to_string())
            .and_modify(|target| *target = (*target).max(target_row_count))
            .or_insert(target_row_count);
        self.request_native_session_catalog_refresh();
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
        self.task_notifier.navigation_changed();

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
            api.native_catalog.set_refreshing(false);
            api.task_notifier.navigation_changed();
            break;
        });
    }

    pub(super) fn refresh_native_session_catalogs(&self) -> Result<(), ProtocolError> {
        let task_records = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?;
        let mut workspaces = self
            .configured_projects
            .projects()
            .into_iter()
            .flat_map(|project| {
                let mut contexts = vec![(
                    project.project_id.as_str().to_string(),
                    project.workspace_root.clone(),
                )];
                if let Ok(Some(repository)) = self
                    .worktrees
                    .refresh_project(std::path::Path::new(&project.workspace_root))
                {
                    contexts.extend(
                        repository
                            .repository
                            .worktrees
                            .into_iter()
                            .filter(|worktree| {
                                !worktree.forgotten
                                    && worktree.availability
                                        == openaide_app_server_protocol::worktree::WorktreeAvailability::Available
                            })
                            .map(|worktree| (project.project_id.as_str().to_string(), worktree.path)),
                    );
                }
                contexts
            })
            .collect::<std::collections::HashSet<_>>();
        workspaces.extend(
            task_records
                .iter()
                .filter(|task| !task.tombstoned)
                .map(|task| {
                    let project_id = ProjectIdentity::from_workspace_root(
                        task.project_root.as_deref().unwrap_or(&task.workspace_root),
                    )
                    .project_id;
                    (project_id.as_str().to_string(), task.workspace_root.clone())
                }),
        );
        let contexts = self
            .agent_registry
            .summaries()
            .into_iter()
            .flat_map(|agent| {
                workspaces
                    .iter()
                    .cloned()
                    .map(move |(project_id, workspace_root)| {
                        (agent.id.clone(), project_id, workspace_root)
                    })
            })
            .collect::<Vec<_>>();
        let mut first_error = None;
        for batch in contexts.chunks(20) {
            let results = std::thread::scope(|scope| {
                batch
                    .iter()
                    .map(|(agent_id, project_id, workspace_root)| {
                        scope.spawn(|| {
                            self.refresh_native_catalog_context(
                                agent_id,
                                project_id,
                                workspace_root,
                                &task_records,
                            )
                        })
                    })
                    .map(|worker| {
                        worker.join().unwrap_or_else(|_| {
                            Err(ProtocolError {
                                code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
                                message: "Native Session refresh worker panicked".to_string(),
                                recoverable: true,
                                target: None,
                            })
                        })
                    })
                    .collect::<Vec<_>>()
            });
            for result in results {
                if let Err(error) = result {
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn refresh_native_catalog_context(
        &self,
        agent_id: &str,
        project_id: &str,
        workspace_root: &str,
        task_records: &[TaskRecord],
    ) -> Result<(), ProtocolError> {
        let target = self
            .native_catalog_refresh
            .state
            .lock()
            .expect("Native Session catalog refresh state poisoned")
            .project_targets
            .get(project_id)
            .copied()
            .unwrap_or(20);
        let mut cursor = OpaqueSessionCursor::new(None);
        let mut sessions = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut listing_order = ListingOrderValidator::new();
        loop {
            let result = self
                .agent_gateway
                .list_sessions(AgentListSessionsRequest {
                    agent_id: agent_id.to_string(),
                    cwd: workspace_root.to_string(),
                    cursor: cursor.current(),
                })
                .map_err(protocol_error_from_runtime)?;
            let page_order = listing_order.observe(&result.sessions, &mut seen);
            self.record_native_catalog_page(
                project_id,
                agent_id,
                workspace_root,
                &result.sessions,
            )?;
            self.reconcile_native_session_activity(
                agent_id,
                workspace_root,
                &result.sessions,
                task_records,
            )?;
            sessions.extend(result.sessions);
            let next = cursor.advance(result.next_cursor);
            let reached_activity_cutoff = listing_order.trusted
                && page_order
                    .activity_frontier
                    .zip(self.project_activity_cutoff(project_id, target, task_records))
                    .is_some_and(|(frontier, cutoff)| frontier <= cutoff);
            if next.is_none()
                || page_order.new_identity_count == 0
                || seen.len() >= target
                || reached_activity_cutoff
            {
                break;
            }
        }
        self.history_sync
            .replace_listed_sessions(agent_id, workspace_root, sessions);
        Ok(())
    }

    fn project_activity_cutoff(
        &self,
        project_id: &str,
        target: usize,
        task_records: &[TaskRecord],
    ) -> Option<i128> {
        if target == 0 {
            return None;
        }
        let enabled_agents = self
            .agent_registry
            .summaries()
            .into_iter()
            .map(|agent| agent.id)
            .collect::<std::collections::HashSet<_>>();
        let owned = task_records
            .iter()
            .filter_map(|task| {
                task.agent_session_id
                    .as_ref()
                    .map(|session_id| (task.agent_id.clone(), session_id.clone()))
            })
            .collect::<std::collections::HashSet<_>>();
        let mut activities = task_records
            .iter()
            .filter(|task| {
                !task.tombstoned
                    && task.lifecycle.is_open()
                    && enabled_agents.contains(&task.agent_id)
                    && ProjectIdentity::from_workspace_root(
                        task.project_root.as_deref().unwrap_or(&task.workspace_root),
                    )
                    .project_id
                    .as_str()
                        == project_id
            })
            .map(|task| crate::time::activity_millis(&task.last_activity))
            .collect::<Vec<_>>();
        activities.extend(
            self.native_catalog
                .entries()
                .into_iter()
                .filter(|entry| entry.project_id == project_id)
                .filter(|entry| enabled_agents.contains(&entry.observation.reference.agent_id))
                .filter(|entry| {
                    !owned.contains(&(
                        entry.observation.reference.agent_id.clone(),
                        entry.observation.reference.session_id.clone(),
                    ))
                })
                .map(|entry| {
                    entry
                        .observation
                        .last_activity
                        .as_deref()
                        .and_then(crate::time::activity_millis)
                }),
        );
        if activities.len() < target {
            return None;
        }
        activities.sort_by(|left, right| right.cmp(left));
        activities.get(target - 1).copied().flatten()
    }

    /// Advances owned Task activity from listings without importing stale runtime metadata.
    fn reconcile_native_session_activity(
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
                    cwd: project.workspace_root.clone(),
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

    fn record_native_catalog_page(
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
        self.task_notifier.navigation_changed();
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

fn listed_session_activity(session: &crate::protocol::model::AgentListedSession) -> Option<i128> {
    session
        .last_activity
        .as_deref()
        .or(session.updated_at.as_deref())
        .and_then(crate::time::activity_millis)
}

#[cfg(test)]
#[path = "list_sessions_tests.rs"]
mod tests;

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
