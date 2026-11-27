//! The one owner of the global state (ADR-0086): projects, worktrees, Agents, settings, New Task
//! defaults, Remote Devices and Terminals.
//!
//! Every change goes through [`GlobalStateOwner::refresh`], which re-reads the areas named by its
//! caller, compares them with what the owner holds, and returns one [`GlobalStateChanges`] per
//! client holding exactly what changed for it. A refresh that changes nothing returns nothing, so
//! a worktree refresh that found the same repository, or a status update that left the Agents as
//! they were, is never published. There is no change log and no sync position: a client that
//! loses its session initializes again and receives the whole state.
//!
//! The owner is reached only by the protocol-lock holder, so a client's `client/initialize`
//! reply and the `sync` notifications after it are one consistent sequence.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::global_state::{GlobalState, GlobalStateChanges};
use openaide_app_server_protocol::ids::{ClientInstanceId, ProjectId, WorktreeRepositoryId};
use openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot;
use serde_json::json;

use crate::logging;
use crate::snapshots::SnapshotBuilder;

/// Which areas of the global state a caller says may have changed. The owner re-reads only those.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GlobalAreas(u8);

impl GlobalAreas {
    /// Projects, and with them the set of worktree repositories they use.
    pub(crate) const PROJECTS: Self = Self(1);
    pub(crate) const AGENTS: Self = Self(1 << 1);
    pub(crate) const SETTINGS: Self = Self(1 << 2);
    pub(crate) const NEW_TASK_DEFAULTS: Self = Self(1 << 3);
    pub(crate) const DEVICES: Self = Self(1 << 4);
    pub(crate) const TERMINALS: Self = Self(1 << 5);
    /// Every repository the owner tracks is read again.
    pub(crate) const WORKTREES: Self = Self(1 << 6);

    pub(crate) fn has_projects_or_agents(self) -> bool {
        self.has(Self::PROJECTS) || self.has(Self::AGENTS)
    }

    fn has(self, area: Self) -> bool {
        self.0 & area.0 != 0
    }

    fn labels(self) -> Vec<&'static str> {
        [
            (Self::PROJECTS, "projects"),
            (Self::AGENTS, "agents"),
            (Self::SETTINGS, "settings"),
            (Self::NEW_TASK_DEFAULTS, "new_task_defaults"),
            (Self::DEVICES, "devices"),
            (Self::TERMINALS, "terminals"),
            (Self::WORKTREES, "worktrees"),
        ]
        .into_iter()
        .filter(|(area, _)| self.has(*area))
        .map(|(_, label)| label)
        .collect()
    }
}

/// A client that receives the global state: all of it, or only what concerns the Projects it
/// works in (VS Code in current-project mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlobalStateAudience {
    pub(crate) client_instance_id: ClientInstanceId,
    /// `Some` limits projects, and the worktree repositories those projects use, to these ids.
    pub(crate) project_filter: Option<Vec<ProjectId>>,
}

#[derive(Default)]
pub(crate) struct GlobalStateOwner {
    state: Option<GlobalState>,
    /// What each filtered client last received, so its changes are exact even when its own
    /// Project set moves with the change.
    filtered_views: HashMap<ClientInstanceId, GlobalState>,
}

impl GlobalStateOwner {
    /// The state a client's `client/initialize` reply carries, filtered for it. A filtered client
    /// is recorded at this state, so the changes that follow are relative to it.
    pub(crate) fn state_for(
        &mut self,
        sources: &SnapshotBuilder,
        audience: &GlobalStateAudience,
    ) -> Result<GlobalState, ProtocolError> {
        let state = self.ensure(sources)?;
        match &audience.project_filter {
            None => {
                self.filtered_views.remove(&audience.client_instance_id);
                Ok(state)
            }
            Some(project_ids) => {
                let view = filter_state(&state, project_ids);
                self.filtered_views
                    .insert(audience.client_instance_id.clone(), view.clone());
                Ok(view)
            }
        }
    }

    /// Re-reads `areas` (and `repository`, a worktree snapshot the caller already holds) and
    /// returns what changed for each of `audiences`; a client with no change is absent.
    pub(crate) fn refresh(
        &mut self,
        sources: &SnapshotBuilder,
        areas: GlobalAreas,
        repository: Option<WorktreeRepositorySnapshot>,
        audiences: &[GlobalStateAudience],
    ) -> Vec<(ClientInstanceId, GlobalStateChanges)> {
        let started = std::time::Instant::now();
        let Some(previous) = self.state.clone() else {
            // Nothing was ever sent from this state, so the first read is a baseline, not a change.
            if let Err(error) = self.ensure(sources) {
                log_read_failed("baseline", &error);
            }
            return Vec::new();
        };
        let next = read_changed_areas(sources, &previous, areas, repository);
        let shared = diff(&previous, &next);
        self.state = Some(next.clone());
        self.filtered_views
            .retain(|client, _| audiences.iter().any(|a| &a.client_instance_id == client));
        let mut deliveries = Vec::new();
        for audience in audiences {
            let changes = match &audience.project_filter {
                None => shared.clone(),
                Some(project_ids) => {
                    let view = filter_state(&next, project_ids);
                    let before = self
                        .filtered_views
                        .insert(audience.client_instance_id.clone(), view.clone())
                        .unwrap_or_else(|| filter_state(&previous, project_ids));
                    diff(&before, &view)
                }
            };
            if !changes.is_empty() {
                deliveries.push((audience.client_instance_id.clone(), changes));
            }
        }
        if !shared.is_empty() || !deliveries.is_empty() {
            logging::info(
                "global_state_published",
                json!({
                    "areas_read": areas.labels(),
                    "changed_area_count": changed_area_count(&shared),
                    "client_count": deliveries.len(),
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
        }
        deliveries
    }

    fn ensure(&mut self, sources: &SnapshotBuilder) -> Result<GlobalState, ProtocolError> {
        if let Some(state) = &self.state {
            return Ok(state.clone());
        }
        let state = sources.read_global_state()?;
        self.state = Some(state.clone());
        Ok(state)
    }
}

/// Reads the named areas into a copy of `previous`. An area that fails to read keeps its previous
/// value and the failure is logged, so one broken source cannot hide the others.
fn read_changed_areas(
    sources: &SnapshotBuilder,
    previous: &GlobalState,
    areas: GlobalAreas,
    repository: Option<WorktreeRepositorySnapshot>,
) -> GlobalState {
    let mut next = previous.clone();
    if areas.has(GlobalAreas::PROJECTS) {
        match sources.project_collection_snapshot() {
            Ok(projects) => next.projects = projects,
            Err(error) => log_read_failed("projects", &error),
        }
    }
    if areas.has(GlobalAreas::AGENTS) {
        match sources.agent_collection_snapshot() {
            Ok(agents) => next.agents = agents,
            Err(error) => log_read_failed("agents", &error),
        }
    }
    if areas.has(GlobalAreas::SETTINGS) {
        match sources.settings_snapshot() {
            Ok(settings) => next.settings = settings,
            Err(error) => log_read_failed("settings", &error),
        }
    }
    if areas.has(GlobalAreas::NEW_TASK_DEFAULTS) {
        match sources.new_task_defaults_snapshot() {
            Ok(defaults) => next.new_task_defaults = defaults,
            Err(error) => log_read_failed("new_task_defaults", &error),
        }
    }
    if areas.has(GlobalAreas::DEVICES) {
        next.devices = sources.device_collection_snapshot();
    }
    if areas.has(GlobalAreas::TERMINALS) {
        next.terminals = sources.terminal_collection_snapshot();
    }
    if areas.has(GlobalAreas::PROJECTS) || areas.has(GlobalAreas::WORKTREES) {
        reconcile_worktrees(sources, &mut next, areas.has(GlobalAreas::WORKTREES));
    }
    if let Some(repository) = repository {
        upsert_repository(&mut next.worktrees, repository);
    }
    next
}

/// Keeps exactly the repositories the Projects use: one that a Project newly uses is read, one no
/// Project uses is dropped, and `reread_all` reads every kept one again.
fn reconcile_worktrees(sources: &SnapshotBuilder, state: &mut GlobalState, reread_all: bool) {
    let wanted: BTreeSet<WorktreeRepositoryId> = state
        .projects
        .projects
        .iter()
        .filter_map(|project| project.worktree_repository_id.clone())
        .collect();
    state
        .worktrees
        .retain(|repository| wanted.contains(&repository.repository_id));
    for repository_id in wanted {
        let known = state
            .worktrees
            .iter()
            .any(|repository| repository.repository_id == repository_id);
        if known && !reread_all {
            continue;
        }
        match sources.worktree_repository(&repository_id) {
            Ok(repository) => upsert_repository(&mut state.worktrees, repository),
            Err(error) => log_read_failed("worktree_repository", &error),
        }
    }
}

fn upsert_repository(
    repositories: &mut Vec<WorktreeRepositorySnapshot>,
    repository: WorktreeRepositorySnapshot,
) {
    match repositories
        .iter_mut()
        .find(|known| known.repository_id == repository.repository_id)
    {
        // The revision counts refreshes, not changes, so it must not make a repeat look new.
        Some(known) if same_repository_content(known, &repository) => {}
        Some(known) => *known = repository,
        None => repositories.push(repository),
    }
    repositories.sort_by(|a, b| a.repository_id.cmp(&b.repository_id));
}

fn same_repository_content(a: &WorktreeRepositorySnapshot, b: &WorktreeRepositorySnapshot) -> bool {
    let mut b = b.clone();
    b.revision = a.revision;
    a == &b
}

/// What changed from `old` to `new`, area by area.
pub(crate) fn diff(old: &GlobalState, new: &GlobalState) -> GlobalStateChanges {
    let old_repositories: BTreeMap<_, _> = old
        .worktrees
        .iter()
        .map(|repository| (&repository.repository_id, repository))
        .collect();
    let new_ids: BTreeSet<_> = new
        .worktrees
        .iter()
        .map(|repository| &repository.repository_id)
        .collect();
    GlobalStateChanges {
        projects: (old.projects != new.projects).then(|| new.projects.clone()),
        worktrees: new
            .worktrees
            .iter()
            .filter(|repository| {
                !old_repositories
                    .get(&repository.repository_id)
                    .is_some_and(|known| same_repository_content(known, repository))
            })
            .cloned()
            .collect(),
        removed_worktree_repositories: old_repositories
            .keys()
            .filter(|id| !new_ids.contains(**id))
            .map(|id| (*id).clone())
            .collect(),
        agents: (old.agents != new.agents).then(|| new.agents.clone()),
        settings: (old.settings != new.settings).then(|| new.settings.clone()),
        new_task_defaults: (old.new_task_defaults != new.new_task_defaults)
            .then(|| new.new_task_defaults.clone()),
        devices: (old.devices != new.devices).then(|| new.devices.clone()),
        terminals: (old.terminals != new.terminals).then(|| new.terminals.clone()),
    }
}

/// The state limited to `project_ids` and the worktree repositories those projects use.
pub(crate) fn filter_state(state: &GlobalState, project_ids: &[ProjectId]) -> GlobalState {
    let mut filtered = state.clone();
    filtered
        .projects
        .projects
        .retain(|project| project_ids.contains(&project.project_id));
    let repositories: BTreeSet<_> = filtered
        .projects
        .projects
        .iter()
        .filter_map(|project| project.worktree_repository_id.clone())
        .collect();
    filtered
        .worktrees
        .retain(|repository| repositories.contains(&repository.repository_id));
    filtered
}

fn changed_area_count(changes: &GlobalStateChanges) -> usize {
    [
        changes.projects.is_some(),
        !changes.worktrees.is_empty() || !changes.removed_worktree_repositories.is_empty(),
        changes.agents.is_some(),
        changes.settings.is_some(),
        changes.new_task_defaults.is_some(),
        changes.devices.is_some(),
        changes.terminals.is_some(),
    ]
    .into_iter()
    .filter(|changed| *changed)
    .count()
}

fn log_read_failed(area: &str, error: &ProtocolError) {
    logging::warn(
        "global_state_read_failed",
        json!({ "area": area, "error_code": format!("{:?}", error.code) }),
    );
}

/// A global state with no projects, Agents or Terminals, for tests that need a valid value.
#[cfg(test)]
pub(crate) fn empty_state_for_tests() -> GlobalState {
    serde_json::from_value(json!({
        "projects": { "projects": [] },
        "agents": { "agents": [] },
        "settings": { "sections": [] },
        "newTaskDefaults": {},
        "devices": {
            "remoteAccess": "off",
            "serverName": "host",
            "devices": [],
        },
        "terminals": { "terminals": [] },
    }))
    .expect("empty global state fixture")
}

#[cfg(test)]
#[path = "global_state_tests.rs"]
mod tests;
