use std::collections::{HashMap, HashSet};
use std::path::Path;

use openaide_app_server_protocol::errors::ProtocolError;

use crate::agent::AgentListSessionsRequest;
use crate::projects::ProjectIdentity;

use super::session_cursor::OpaqueSessionCursor;
use super::{protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    /// Discovers sessions for an added Project, including nested working folders.
    ///
    /// ACP's cwd filter is exact, so Project-tree discovery requests the global
    /// catalog and assigns every session to its most specific visible Project.
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
        let mut workspace_owners = projects
            .iter()
            .map(|project| ProjectWorkspaceOwner {
                project_id: project.project_id.as_str().to_string(),
                workspace_root: project.workspace_root.clone(),
            })
            .collect::<Vec<_>>();
        for project in &projects {
            if let Ok(Some(repository)) = self
                .worktrees
                .refresh_project(Path::new(&project.workspace_root))
            {
                workspace_owners.extend(
                    repository
                        .repository
                        .worktrees
                        .into_iter()
                        .filter(|worktree| {
                            !worktree.forgotten
                                && worktree.availability
                                    == openaide_app_server_protocol::worktree::WorktreeAvailability::Available
                        })
                        .map(|worktree| ProjectWorkspaceOwner {
                            project_id: project.project_id.as_str().to_string(),
                            workspace_root: worktree.path,
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
            workspace_owners.push(ProjectWorkspaceOwner {
                project_id: project.project_id.as_str().to_string(),
                workspace_root: project.workspace_root,
            });
            workspace_owners.push(ProjectWorkspaceOwner {
                project_id: project.project_id.as_str().to_string(),
                workspace_root: task.workspace_root.clone(),
            });
        }
        let mut selected_project_ids = projects
            .iter()
            .filter(|project| {
                project_filter.is_none_or(|filter| project.project_id.as_str() == filter)
            })
            .map(|project| project.project_id.as_str().to_string())
            .collect::<HashSet<_>>();
        selected_project_ids.extend(
            workspace_owners
                .iter()
                .filter(|owner| project_filter.is_none_or(|filter| owner.project_id == filter))
                .map(|owner| owner.project_id.clone()),
        );
        if project_filter.is_some_and(|project_id| !selected_project_ids.contains(project_id)) {
            return Err(super::internal_error(
                "Project disappeared during session discovery",
            ));
        }
        let mut discovered_counts = HashMap::<String, usize>::new();
        let mut stopped_with_more = false;
        let agents = self.agent_registry.summaries();

        for (agent_index, agent) in agents.iter().enumerate() {
            let mut cursor = OpaqueSessionCursor::new(None);
            let mut seen_session_ids = std::collections::HashSet::new();
            let mut sessions_by_workspace = HashMap::<(String, String), Vec<_>>::new();
            loop {
                let result = self
                    .agent_gateway
                    .list_sessions(AgentListSessionsRequest {
                        agent_id: agent.id.clone(),
                        cwd: None,
                        cursor: cursor.current(),
                    })
                    .map_err(protocol_error_from_runtime)?;
                let mut new_identity_count = 0_usize;
                let mut page_by_workspace = HashMap::<(String, String), Vec<_>>::new();
                for session in result.sessions {
                    if !seen_session_ids.insert(session.session_id.clone()) {
                        continue;
                    }
                    new_identity_count = new_identity_count.saturating_add(1);
                    if let Some(owner) =
                        owning_project(&workspace_owners, &session.cwd).filter(|owner| {
                            project_filter.is_none_or(|filter| owner.project_id == filter)
                        })
                    {
                        page_by_workspace
                            .entry((owner.project_id.clone(), session.cwd.clone()))
                            .or_default()
                            .push(session);
                    }
                }
                for ((project_id, workspace_root), sessions) in page_by_workspace {
                    discovered_counts
                        .entry(project_id.clone())
                        .and_modify(|count| *count = count.saturating_add(sessions.len()))
                        .or_insert(sessions.len());
                    self.record_native_catalog_page(
                        &project_id,
                        &agent.id,
                        &workspace_root,
                        &sessions,
                    )?;
                    sessions_by_workspace
                        .entry((project_id, workspace_root))
                        .or_default()
                        .extend(sessions);
                }
                let next_cursor = cursor.advance(result.next_cursor);
                if new_identity_count == 0 {
                    break;
                }
                if target_row_count.is_some_and(|target| {
                    selected_project_ids.iter().all(|project_id| {
                        self.active_navigation_row_count(project_id, &task_records) >= target
                    })
                }) {
                    stopped_with_more = next_cursor.is_some() || agent_index + 1 < agents.len();
                    break;
                }
                if next_cursor.is_none() {
                    break;
                }
            }

            for ((_project_id, workspace_root), sessions) in sessions_by_workspace {
                self.reconcile_native_session_activity(
                    &agent.id,
                    &workspace_root,
                    &sessions,
                    &task_records,
                )?;
            }
            if stopped_with_more {
                break;
            }
        }

        for project_id in selected_project_ids {
            if self
                .native_catalog
                .set_project_has_more(&project_id, stopped_with_more)
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

    fn active_navigation_row_count(
        &self,
        project_id: &str,
        task_records: &[crate::storage::records::TaskRecord],
    ) -> usize {
        let enabled_agents = self
            .agent_registry
            .summaries()
            .into_iter()
            .map(|agent| agent.id)
            .collect::<HashSet<_>>();
        let owned = task_records
            .iter()
            .filter_map(|task| {
                task.agent_session_id
                    .as_ref()
                    .map(|session_id| (task.agent_id.clone(), session_id.clone()))
            })
            .collect::<HashSet<_>>();
        let task_count = task_records
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
            .count();
        let session_count = self
            .native_catalog
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
            .filter(|entry| {
                !self
                    .native_catalog
                    .is_archived(&entry.observation.reference)
            })
            .count();
        task_count + session_count
    }
}

struct ProjectWorkspaceOwner {
    project_id: String,
    workspace_root: String,
}

fn owning_project<'a>(
    projects: &'a [ProjectWorkspaceOwner],
    workspace_root: &str,
) -> Option<&'a ProjectWorkspaceOwner> {
    let workspace_root = Path::new(workspace_root);
    projects
        .iter()
        .filter(|project| workspace_root.starts_with(Path::new(&project.workspace_root)))
        .max_by_key(|project| Path::new(&project.workspace_root).components().count())
}
