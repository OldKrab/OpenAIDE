use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use openaide_app_server_protocol::errors::ProtocolError;

use crate::agent::AgentListSessionsRequest;
use crate::native_sessions::catalog::NativeSessionRef;
use crate::projects::ProjectIdentity;

use super::session_cursor::OpaqueSessionCursor;
use super::{protocol_error_from_runtime, TaskProductApi};

/// Bounds passive refresh work while covering the largest initial per-Project window.
const INITIAL_NATIVE_SESSION_ROW_TARGET: usize = 20;

impl TaskProductApi {
    /// Discovers a bounded window for every known Project workspace and enabled Agent.
    ///
    /// ACP's cwd filter is exact, so roots, available worktrees, and historical Task
    /// workspaces are independent discovery contexts. This avoids making a sparse Project
    /// paginate through another Project's entire Agent history.
    pub(super) fn refresh_native_session_project_trees(
        &self,
        project_filter: Option<&str>,
        target_row_count: Option<usize>,
    ) -> Result<(), ProtocolError> {
        let projects = self.configured_projects.projects();
        let task_records = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?;
        let mut project_workspaces = projects
            .iter()
            .map(|project| {
                (
                    project.project_id.as_str().to_string(),
                    project.workspace_root.clone(),
                )
            })
            .collect::<HashSet<_>>();
        for project in &projects {
            if let Ok(Some(repository)) = self
                .worktrees
                .refresh_project(Path::new(&project.workspace_root))
            {
                project_workspaces.extend(
                    repository
                        .repository
                        .worktrees
                        .into_iter()
                        .filter(|worktree| {
                            !worktree.forgotten
                                && worktree.availability
                                    == openaide_app_server_protocol::worktree::WorktreeAvailability::Available
                        })
                        .map(|worktree| {
                            (project.project_id.as_str().to_string(), worktree.path)
                        }),
                );
            }
        }
        // Historical Tasks remain valid discovery roots even when no shell is
        // currently advertising their Project. Explicitly removed Projects are
        // excluded so a refresh cannot resurrect them.
        for task in task_records.iter().filter(|task| !task.tombstoned) {
            let project_root = task.project_root.as_deref().unwrap_or(&task.workspace_root);
            let project = ProjectIdentity::from_workspace_root(project_root);
            if self.configured_projects.is_removed(&project.project_id) {
                continue;
            }
            project_workspaces.insert((
                project.project_id.as_str().to_string(),
                project.workspace_root,
            ));
            project_workspaces.insert((
                project.project_id.as_str().to_string(),
                task.workspace_root.clone(),
            ));
        }
        let mut selected_project_ids = projects
            .iter()
            .filter(|project| {
                project_filter.is_none_or(|filter| project.project_id.as_str() == filter)
            })
            .map(|project| project.project_id.as_str().to_string())
            .collect::<HashSet<_>>();
        selected_project_ids.extend(
            project_workspaces
                .iter()
                .filter(|(project_id, _)| project_filter.is_none_or(|filter| project_id == filter))
                .map(|(project_id, _)| project_id.clone()),
        );
        if project_filter.is_some_and(|project_id| !selected_project_ids.contains(project_id)) {
            return Err(super::internal_error(
                "Project disappeared during session discovery",
            ));
        }
        let agents = self.agent_registry.summaries();
        let target_row_count = target_row_count.unwrap_or(INITIAL_NATIVE_SESSION_ROW_TARGET);
        let selected_workspaces = project_workspaces
            .iter()
            .filter(|(project_id, _)| project_filter.is_none_or(|filter| project_id == filter))
            .cloned()
            .collect::<Vec<_>>();
        let mut outcomes = Vec::with_capacity(agents.len() * selected_workspaces.len());
        let mut first_error = None;
        let agent_results = std::thread::scope(|scope| {
            agents
                .iter()
                .map(|agent| {
                    scope.spawn(|| {
                        let mut agent_outcomes = Vec::with_capacity(selected_workspaces.len());
                        let mut agent_error = None;
                        for (project_id, workspace_root) in &selected_workspaces {
                            match self.refresh_native_session_context(
                                &agent.id,
                                project_id,
                                workspace_root,
                                target_row_count,
                                &task_records,
                            ) {
                                Ok(outcome) => agent_outcomes.push(outcome),
                                Err(error) => {
                                    agent_error.get_or_insert(error);
                                }
                            }
                        }
                        (agent_outcomes, agent_error)
                    })
                })
                .map(|worker| worker.join())
                .collect::<Vec<_>>()
        });
        for result in agent_results {
            match result {
                Ok((agent_outcomes, agent_error)) => {
                    outcomes.extend(agent_outcomes);
                    if let Some(error) = agent_error {
                        first_error.get_or_insert(error);
                    }
                }
                Err(_) => {
                    first_error.get_or_insert_with(|| {
                        super::internal_error("Native Session refresh worker panicked")
                    });
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }

        let mut discovered_counts = HashMap::<String, usize>::new();
        let mut projects_with_more = HashSet::<String>::new();
        for outcome in outcomes {
            discovered_counts
                .entry(outcome.project_id.clone())
                .and_modify(|count| *count = count.saturating_add(outcome.discovered_count))
                .or_insert(outcome.discovered_count);
            if outcome.has_more {
                projects_with_more.insert(outcome.project_id);
            }
        }
        self.native_catalog_refresh.mark_projects_exhausted(
            selected_project_ids
                .iter()
                .filter(|project_id| !projects_with_more.contains(*project_id)),
        );
        for project_id in selected_project_ids {
            let has_more = projects_with_more.contains(&project_id);
            if self
                .native_catalog
                .set_project_has_more(&project_id, has_more)
            {
                self.task_notifier
                    .navigation_project_entries_changed(project_id.clone());
            }
            crate::logging::info(
                "native_session_project_tree_refreshed",
                serde_json::json!({
                    "project_id": project_id,
                    "discovered_count": discovered_counts.get(&project_id).copied().unwrap_or(0),
                }),
            );
        }
        Ok(())
    }

    fn refresh_native_session_context(
        &self,
        agent_id: &str,
        project_id: &str,
        workspace_root: &str,
        target_row_count: usize,
        task_records: &[crate::storage::records::TaskRecord],
    ) -> Result<NativeSessionContextRefresh, ProtocolError> {
        let started_at = Instant::now();
        crate::logging::info(
            "native_session_context_refresh_started",
            serde_json::json!({
                "operation": "agent/list_sessions/context",
                "agent_id": agent_id,
                "project_id": project_id,
                "target_row_count": target_row_count,
            }),
        );
        let owned_sessions = task_records
            .iter()
            .filter_map(|task| {
                task.agent_session_id
                    .as_ref()
                    .map(|session_id| (task.agent_id.as_str(), session_id.as_str()))
            })
            .collect::<HashSet<_>>();
        let mut cursor = OpaqueSessionCursor::new(None);
        let mut seen_session_ids = HashSet::new();
        let mut visible_session_ids = HashSet::new();
        let mut page_count = 0_usize;
        let mut has_more = false;
        loop {
            let result = match self.agent_gateway.list_sessions(AgentListSessionsRequest {
                agent_id: agent_id.to_string(),
                cwd: Some(workspace_root.to_string()),
                cursor: cursor.current(),
            }) {
                Ok(result) => result,
                Err(error) => {
                    crate::logging::warn(
                        "native_session_context_refresh_failed",
                        serde_json::json!({
                            "operation": "agent/list_sessions/context",
                            "agent_id": agent_id,
                            "project_id": project_id,
                            "duration_ms": started_at.elapsed().as_millis(),
                            "page_count": page_count,
                            "error_kind": error.code(),
                        }),
                    );
                    return Err(protocol_error_from_runtime(error));
                }
            };
            page_count = page_count.saturating_add(1);
            let mut new_identity_count = 0_usize;
            for session in &result.sessions {
                if !seen_session_ids.insert(session.session_id.clone()) {
                    continue;
                }
                new_identity_count = new_identity_count.saturating_add(1);
                let reference = NativeSessionRef::new(agent_id, &session.session_id);
                if !owned_sessions.contains(&(agent_id, session.session_id.as_str()))
                    && !self.native_catalog.is_archived(&reference)
                {
                    visible_session_ids.insert(session.session_id.clone());
                }
            }
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
            let next_cursor = cursor.advance(result.next_cursor);
            if new_identity_count == 0 {
                break;
            }
            if visible_session_ids.len() >= target_row_count {
                has_more = next_cursor.is_some();
                break;
            }
            if next_cursor.is_none() {
                break;
            }
        }
        crate::logging::info(
            "native_session_context_refresh_completed",
            serde_json::json!({
                "operation": "agent/list_sessions/context",
                "agent_id": agent_id,
                "project_id": project_id,
                "duration_ms": started_at.elapsed().as_millis(),
                "page_count": page_count,
                "observed_session_count": seen_session_ids.len(),
                "visible_session_count": visible_session_ids.len(),
                "has_more": has_more,
            }),
        );
        Ok(NativeSessionContextRefresh {
            project_id: project_id.to_string(),
            discovered_count: seen_session_ids.len(),
            has_more,
        })
    }

    pub(super) fn initial_native_session_row_target() -> usize {
        INITIAL_NATIVE_SESSION_ROW_TARGET
    }
}

struct NativeSessionContextRefresh {
    project_id: String,
    discovered_count: usize,
    has_more: bool,
}
