use openaide_app_server_protocol::agent::{
    AgentListSessionsParams, AgentListSessionsResult, AgentListedSession,
};
use openaide_app_server_protocol::errors::ProtocolError;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use crate::agent::{AgentListSessionsRequest, AgentSessionKey};
use crate::native_sessions::catalog::{NativeSessionObservation, NativeSessionRef};
use crate::protocol::model::TaskStatus;
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};

use super::session_cursor::OpaqueSessionCursor;
use super::{protocol_error_from_runtime, AgentListSessionsWorkflow, TaskProductApi};

#[derive(Clone, Default)]
pub(super) struct NativeCatalogRefreshCoordinator {
    state: Arc<Mutex<NativeCatalogRefreshState>>,
    listing_slots: Arc<(Mutex<usize>, Condvar)>,
}

pub(super) const MAX_CONCURRENT_CATALOG_LISTINGS: usize = 20;

/// How long after an App Server write the Agent's listed timestamp may still trail it.
const OWN_ACTIVITY_TOLERANCE_MS: i128 = 5_000;

/// A permit spans one Agent request; unwinding also releases capacity.
struct CatalogListingPermit<'a>(&'a (Mutex<usize>, Condvar));

impl Drop for CatalogListingPermit<'_> {
    fn drop(&mut self) {
        *self.0 .0.lock().expect("catalog listing slots poisoned") -= 1;
        self.0 .1.notify_one();
    }
}

#[derive(Default)]
struct NativeCatalogRefreshState {
    exhausted_project_ids: HashSet<String>,
    // TODO: unify global and Project demand by Agent plus canonical workspace;
    // separate coordinators can still repeat a context during overlapping refreshes.
    project_targets: HashMap<String, usize>,
    running: bool,
    trailing_run_requested: bool,
}

impl NativeCatalogRefreshCoordinator {
    /// All discovery entry points share the same process-wide request budget.
    pub(super) fn list_sessions(
        &self,
        gateway: &crate::agent::gateway::AgentGateway,
        request: AgentListSessionsRequest,
    ) -> Result<
        crate::protocol::model::AgentListSessionsResult,
        crate::protocol::errors::RuntimeError,
    > {
        let started_at = Instant::now();
        let operation_id = request.operation_id.clone();
        let agent_id = request.agent_id.clone();
        crate::logging::info(
            "native_session_page_requested",
            serde_json::json!({
                "operation": "agent/list_sessions/page", "operation_id": operation_id,
                "agent_id": agent_id, "attempt": 1, "has_cursor": request.cursor.is_some(),
            }),
        );
        let (active, available) = &*self.listing_slots;
        let mut active = available
            .wait_while(
                active.lock().expect("catalog listing slots poisoned"),
                |active| *active >= MAX_CONCURRENT_CATALOG_LISTINGS,
            )
            .expect("catalog listing slots poisoned");
        *active += 1;
        drop(active);
        let _permit = CatalogListingPermit(&self.listing_slots);
        let queue_ms = started_at.elapsed().as_millis();
        let result = gateway.list_sessions(request);
        let fields = serde_json::json!({
            "operation": "agent/list_sessions/page", "operation_id": operation_id,
            "agent_id": agent_id, "attempt": 1,
            "outcome": if result.is_ok() { "completed" } else { "failed" },
            "queue_ms": queue_ms, "duration_ms": started_at.elapsed().as_millis(),
            "returned_count": result.as_ref().ok().map(|page| page.sessions.len()),
            "authoritative": result.as_ref().ok().map(|page| page.authoritative),
            "error_kind": result.as_ref().err().map(|error| error.reason()),
        });
        if result.is_ok() {
            crate::logging::info("native_session_page_completed", fields);
        } else {
            crate::logging::warn("native_session_page_failed", fields);
        }
        result
    }

    fn begin_ordinary_refresh(&self) {
        self.state
            .lock()
            .expect("Native Session catalog refresh state poisoned")
            .exhausted_project_ids
            .clear();
    }

    fn project_is_exhausted(&self, project_id: &str) -> bool {
        self.state
            .lock()
            .expect("Native Session catalog refresh state poisoned")
            .exhausted_project_ids
            .contains(project_id)
    }

    pub(super) fn mark_projects_exhausted<'a>(
        &self,
        project_ids: impl IntoIterator<Item = &'a String>,
    ) {
        self.state
            .lock()
            .expect("Native Session catalog refresh state poisoned")
            .exhausted_project_ids
            .extend(project_ids.into_iter().cloned());
    }
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
        self.native_catalog_refresh.begin_ordinary_refresh();
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
            let operation_id = uuid::Uuid::new_v4().to_string();
            crate::logging::info(
                "native_session_catalog_refresh_started",
                serde_json::json!({ "operation": "agent/list_sessions", "operation_id": operation_id, "attempt": 1 }),
            );
            let refresh = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                api.refresh_native_session_project_trees_with_id(
                    None,
                    Some(Self::initial_native_session_row_target()),
                    &operation_id,
                )
            }))
            .unwrap_or_else(|_| {
                Err(super::internal_error(
                    "Native Session refresh worker failed",
                ))
            });
            let mut state = api
                .native_catalog_refresh
                .state
                .lock()
                .expect("Native Session catalog refresh state poisoned");
            let refresh = match refresh {
                Ok(()) => {
                    crate::logging::info(
                        "native_session_catalog_refresh_completed",
                        serde_json::json!({
                            "operation": "agent/list_sessions",
                            "operation_id": operation_id,
                            "attempt": 1,
                            "outcome": "completed",
                            "trailing_run_requested": state.trailing_run_requested,
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
                            "operation_id": operation_id,
                            "attempt": 1,
                            "outcome": "failed",
                            "trailing_run_requested": state.trailing_run_requested,
                            "duration_ms": started_at.elapsed().as_millis(),
                            "error_kind": error.code,
                        }),
                    );
                    openaide_app_server_protocol::snapshot::TaskNavigationRefreshState::Failed {
                        message: error.message,
                    }
                }
            };
            if state.trailing_run_requested {
                state.trailing_run_requested = false;
                continue;
            }
            state.running = false;
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
        // An exhaustive scan is authoritative until the next ordinary refresh. Archive and
        // Restore only reclassify cached entries, so repeating Agent pagination cannot reveal
        // another row and is particularly expensive for large histories.
        if self
            .native_catalog_refresh
            .project_is_exhausted(project_id.as_str())
        {
            crate::logging::info(
                "native_session_project_catalog_refresh_skipped",
                serde_json::json!({
                    "operation": "agent/list_sessions/project",
                    "project_id": project_id.as_str(),
                    "reason": "history_exhausted",
                    "target_row_count": target_row_count,
                }),
            );
            if self
                .native_catalog
                .set_project_has_more(project_id.as_str(), false)
            {
                self.task_notifier
                    .navigation_project_entries_changed(project_id.as_str().to_string());
            }
            return;
        }
        {
            let mut state = self
                .native_catalog_refresh
                .state
                .lock()
                .expect("Native Session catalog refresh state poisoned");
            if let Some(target) = state.project_targets.get_mut(project_id.as_str()) {
                *target = (*target).max(target_row_count);
                return;
            }
            state
                .project_targets
                .insert(project_id.as_str().to_string(), target_row_count);
        }
        if self
            .native_catalog
            .set_project_refreshing(project_id.as_str(), true)
        {
            self.task_notifier
                .navigation_project_entries_changed(project_id.as_str().to_string());
        }
        let api = self.clone();
        std::thread::spawn(move || loop {
            let target_row_count = api
                .native_catalog_refresh
                .state
                .lock()
                .expect("Native Session catalog refresh state poisoned")
                .project_targets[project_id.as_str()];
            let started_at = Instant::now();
            let operation_id = uuid::Uuid::new_v4().to_string();
            crate::logging::info(
                "native_session_project_catalog_refresh_started",
                serde_json::json!({
                    "operation": "agent/list_sessions/project",
                    "operation_id": operation_id, "attempt": 1,
                    "project_id": project_id.as_str(),
                    "target_row_count": target_row_count,
                }),
            );
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                api.refresh_native_session_project_trees_with_id(
                    Some(project_id.as_str()),
                    Some(target_row_count),
                    &operation_id,
                )
            }))
            .unwrap_or_else(|_| {
                Err(super::internal_error(
                    "Native Session refresh worker failed",
                ))
            });
            let outcome = if let Err(error) = result {
                crate::logging::warn(
                    "native_session_project_catalog_refresh_failed",
                    serde_json::json!({
                        "project_id": project_id.as_str(),
                        "operation_id": operation_id, "attempt": 1,
                        "error_kind": error.code,
                    }),
                );
                "failed"
            } else {
                "completed"
            };
            crate::logging::info(
                "native_session_project_catalog_refresh_completed",
                serde_json::json!({
                    "operation": "agent/list_sessions/project",
                    "operation_id": operation_id, "attempt": 1,
                    "project_id": project_id.as_str(),
                    "target_row_count": target_row_count,
                    "outcome": outcome,
                    "duration_ms": started_at.elapsed().as_millis(),
                }),
            );
            let mut state = api
                .native_catalog_refresh
                .state
                .lock()
                .expect("Native Session catalog refresh state poisoned");
            if state.project_targets[project_id.as_str()] > target_row_count {
                continue;
            }
            state.project_targets.remove(project_id.as_str());
            api.native_catalog
                .set_project_refreshing(project_id.as_str(), false);
            api.task_notifier
                .navigation_project_entries_changed(project_id.as_str().to_string());
            break;
        });
    }

    #[cfg(test)]
    pub(super) fn refresh_native_session_catalogs(&self) -> Result<(), ProtocolError> {
        self.refresh_native_session_project_trees(
            None,
            Some(Self::initial_native_session_row_target()),
        )
    }

    /// Advances idle owned Task activity from listing and records possible external changes
    /// without replacing an active attachment. In-progress Tasks keep the OpenAIDE-owned
    /// activity clock: listing often reports "now" as a live heartbeat, and applying it
    /// reshuffles Navigation among concurrent in-progress Tasks. Catalog timestamps cannot
    /// distinguish history from option/title changes, so replay remains an explicit Task action.
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
            let native_time = native_activity
                .as_deref()
                .and_then(crate::time::activity_millis);
            let local_history_time = self
                .store
                .local_history_updated_at(&record.task_id)
                .ok()
                .as_deref()
                .and_then(crate::time::activity_millis);
            let reload_requirement_changed = std::cell::Cell::new(false);
            let own_operation_time = std::cell::Cell::new(None);
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
                    // The App Server's own writes are the baseline: history it stored and
                    // session requests it issued. Agent activity shortly after either is
                    // that Agent's bookkeeping for them, not a change made elsewhere.
                    let own_time = task
                        .native_session_own_activity_at
                        .as_deref()
                        .and_then(crate::time::activity_millis);
                    own_operation_time.set(own_time);
                    let baseline = local_history_time.max(own_time);
                    let exceeds_baseline = native_time.zip(baseline).map(|(native, baseline)| {
                        native > baseline.saturating_add(OWN_ACTIVITY_TOLERANCE_MS)
                    });
                    let mut changed = false;
                    if let Some(native_activity) = &native_activity {
                        if !task_has_live_work(task)
                            && exceeds_baseline != Some(false)
                            && crate::time::activity_millis(native_activity)
                                .zip(crate::time::activity_millis(&task.last_activity))
                                .is_some_and(|(native, current)| native > current)
                        {
                            task.last_activity = native_activity.clone();
                            changed = true;
                        }
                    }
                    if matches!(task.lifecycle, TaskLifecycle::Open)
                        && matches!(task.status, TaskStatus::Inactive)
                        && task.active_turn_id.is_none()
                        && exceeds_baseline == Some(true)
                        && native_activity.as_ref().is_some_and(|activity| {
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
                // The gaps show whether a wrong report came from an App Server operation
                // that was never recorded or from Agent activity outside the tolerance.
                crate::logging::info(
                    "native_session_external_activity_detected",
                    serde_json::json!({
                        "task_id": record.task_id,
                        "agent_id": agent_id,
                        "session_id": expected_session_id,
                        "since_own_operation_ms": native_time
                            .zip(own_operation_time.get())
                            .map(|(native, own)| (native - own).to_string()),
                        "since_local_history_ms": native_time
                            .zip(local_history_time)
                            .map(|(native, local)| (native - local).to_string()),
                    }),
                );
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
        let generation = self.native_catalog.observation_generation();
        loop {
            let result = self
                .native_catalog_refresh
                .list_sessions(
                    &self.agent_gateway,
                    AgentListSessionsRequest {
                        operation_id: uuid::Uuid::new_v4().to_string(),
                        agent_id: params.agent_id.as_str().to_string(),
                        cwd: Some(project.workspace_root.clone()),
                        cursor: cursor.current(),
                    },
                )
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
                generation,
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
        generation: u64,
    ) -> Result<(), ProtocolError> {
        self.native_catalog
            .record_page_from_scan(
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
                Some(generation),
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

fn task_has_live_work(task: &TaskRecord) -> bool {
    task.active_turn_id.is_some()
        || matches!(
            task.status,
            TaskStatus::Starting | TaskStatus::Active | TaskStatus::Waiting | TaskStatus::Stopping
        )
}
