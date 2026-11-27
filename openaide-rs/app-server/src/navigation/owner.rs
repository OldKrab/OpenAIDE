use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{ClientInstanceId, ProjectId};
use openaide_app_server_protocol::navigation::{NavigationFrame, NavigationViewParams};
use openaide_app_server_protocol::task::TaskNavigationSection;

use super::dirty::NavigationDirty;
use super::rows::{NavigationSource, ProjectRows};
use super::view::View;
use crate::native_sessions::discovery_state::DiscoveryState;

/// A recompute this slow explains Navigation latency; healthy flushes stay quiet.
const SLOW_FLUSH: Duration = Duration::from_millis(50);

/// Frames one flush produced, in the order they must be published.
#[derive(Default)]
pub(crate) struct FlushedFrames {
    pub(crate) frames: Vec<(ClientInstanceId, NavigationFrame)>,
    /// Projects whose Tasks-view row target grew, which discovery must deepen.
    pub(crate) deepen: Vec<(ProjectId, usize)>,
}

#[derive(Default)]
struct SectionRows {
    projects: HashMap<ProjectId, ProjectRows>,
    /// False until the section has been read in full once.
    loaded: bool,
}

/// Serves every client's Navigation views. Only the protocol lock holder may call it.
pub(crate) struct NavigationOwner {
    source: Arc<dyn NavigationSource>,
    dirty: NavigationDirty,
    discovery: DiscoveryState,
    views: HashMap<(ClientInstanceId, TaskNavigationSection), View>,
    sections: HashMap<TaskNavigationSection, SectionRows>,
    /// Stamped on each frame produced (see `NavigationFrame::revision`).
    revision: u64,
}

impl NavigationOwner {
    pub(crate) fn new(
        source: Arc<dyn NavigationSource>,
        dirty: NavigationDirty,
        discovery: DiscoveryState,
    ) -> Self {
        Self {
            source,
            dirty,
            discovery,
            views: HashMap::new(),
            sections: HashMap::new(),
            revision: 0,
        }
    }

    /// The revision of the latest frame produced; a baseline with no changes carries it.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// An owner over no Projects, for gateways built without a Task store.
    pub(crate) fn empty() -> Self {
        Self::new(
            Arc::new(EmptySource),
            NavigationDirty::default(),
            DiscoveryState::default(),
        )
    }

    /// For handlers whose mutation changed one Project's rows. The flush before the reply
    /// publishes the frames, so the handler publishes nothing itself.
    pub(crate) fn mark_project(&self, project_id: ProjectId) {
        self.dirty.mark_project(project_id);
    }

    /// For changes that can touch every Project: the Project or Agent collections.
    pub(crate) fn mark_all(&self) {
        self.dirty.mark_all();
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty.is_dirty()
    }

    pub(crate) fn has_tasks_view(&self) -> bool {
        self.views
            .keys()
            .any(|(_, section)| *section == TaskNavigationSection::Tasks)
    }

    /// Registers or re-parameterizes a view and recomputes. The returned frames include the
    /// view's own diff (its full baseline when `baseline`), so the caller can answer with it.
    pub(crate) fn upsert_view(
        &mut self,
        client: &ClientInstanceId,
        params: NavigationViewParams,
        baseline: bool,
    ) -> Result<FlushedFrames, ProtocolError> {
        let key = (client.clone(), params.section);
        let view = self
            .views
            .entry(key)
            .or_insert_with(|| View::new(params.clone()));
        view.params = params;
        if baseline {
            view.reset();
        }
        self.recompute()
    }

    pub(crate) fn remove_view(
        &mut self,
        client: &ClientInstanceId,
        section: TaskNavigationSection,
    ) {
        self.views.remove(&(client.clone(), section));
        self.update_demand();
    }

    pub(crate) fn drop_client(&mut self, client: &ClientInstanceId) {
        self.views.retain(|(owner, _), _| owner != client);
        self.update_demand();
    }

    /// Recomputes the dirty Projects once and diffs every view. Many marks cost one recompute.
    pub(crate) fn flush(&mut self) -> FlushedFrames {
        let started_at = Instant::now();
        match self.recompute() {
            Ok(flushed) => {
                let elapsed = started_at.elapsed();
                if elapsed >= SLOW_FLUSH {
                    crate::logging::info(
                        "navigation_flush_slow",
                        serde_json::json!({
                            "operation": "navigation_flush",
                            "outcome": "completed",
                            "view_count": self.views.len(),
                            "frame_count": flushed.frames.len(),
                            "duration_ms": elapsed.as_millis(),
                        }),
                    );
                }
                flushed
            }
            Err(error) => {
                crate::logging::warn(
                    "navigation_recompute_failed",
                    serde_json::json!({
                        "operation": "navigation_flush",
                        "outcome": "failed",
                        "error_code": format!("{:?}", error.code),
                    }),
                );
                FlushedFrames::default()
            }
        }
    }

    fn recompute(&mut self) -> Result<FlushedFrames, ProtocolError> {
        if let Err(error) = self.refresh_rows() {
            // The marks were consumed; keep them so the next flush retries.
            self.dirty.mark_all();
            return Err(error);
        }
        let keys: Vec<_> = self.views.keys().cloned().collect();
        let mut frames = Vec::new();
        for key in keys {
            if let Some(frame) = self.diff_view(&key) {
                frames.push((key.0, frame));
            }
        }
        let deepen = self.update_demand();
        Ok(FlushedFrames { frames, deepen })
    }

    /// Rebuilds the sorted rows the dirty marks invalidate, for sections that have views.
    fn refresh_rows(&mut self) -> Result<(), ProtocolError> {
        let marks = self.dirty.take();
        let wanted: HashSet<TaskNavigationSection> =
            self.views.keys().map(|(_, section)| *section).collect();
        for section in [TaskNavigationSection::Tasks, TaskNavigationSection::Archive] {
            let cache = self.sections.entry(section).or_default();
            if !wanted.contains(&section) {
                // Nobody watches it, so a mark would be stale by the time somebody does.
                if !marks.is_empty() {
                    cache.loaded = false;
                    cache.projects.clear();
                }
                continue;
            }
            if marks.all || !cache.loaded {
                let groups = self.source.groups(section, None)?;
                cache.projects = groups
                    .into_iter()
                    .map(|group| (group.project_id.clone(), ProjectRows::build(group)))
                    .collect();
                cache.loaded = true;
            } else if !marks.projects.is_empty() {
                let groups = self.source.groups(section, Some(&marks.projects))?;
                for project in &marks.projects {
                    cache.projects.remove(project);
                }
                for group in groups {
                    cache
                        .projects
                        .insert(group.project_id.clone(), ProjectRows::build(group));
                }
            }
        }
        Ok(())
    }

    fn diff_view(
        &mut self,
        key: &(ClientInstanceId, TaskNavigationSection),
    ) -> Option<NavigationFrame> {
        let rows = &self.sections.get(&key.1)?.projects;
        let source = &self.source;
        let mut frame = self
            .views
            .get_mut(key)?
            .diff(rows, |project_id| source.discovery(project_id))?;
        self.revision += 1;
        frame.revision = self.revision;
        Some(frame)
    }

    /// Replaces the demand board from the Tasks views: the largest history limit any of them
    /// applies to each Project they show. The Archive never drives discovery.
    fn update_demand(&mut self) -> Vec<(ProjectId, usize)> {
        let mut demand = HashMap::<String, usize>::new();
        if let Some(rows) = self.sections.get(&TaskNavigationSection::Tasks) {
            for view in self
                .views
                .values()
                .filter(|view| view.section() == TaskNavigationSection::Tasks)
            {
                for project_id in rows.projects.keys() {
                    if view
                        .params
                        .project_ids
                        .as_ref()
                        .is_some_and(|ids| !ids.contains(project_id))
                    {
                        continue;
                    }
                    let target = view.history_limit(project_id);
                    let entry = demand.entry(project_id.as_str().to_string()).or_default();
                    *entry = (*entry).max(target);
                }
            }
        }
        self.discovery
            .replace_demand(demand)
            .into_iter()
            .map(|(project, target)| (ProjectId::from(project), target))
            .collect()
    }
}

struct EmptySource;

impl NavigationSource for EmptySource {
    fn groups(
        &self,
        _section: TaskNavigationSection,
        _projects: Option<&HashSet<ProjectId>>,
    ) -> Result<Vec<super::rows::SourceGroup>, ProtocolError> {
        Ok(Vec::new())
    }

    fn discovery(
        &self,
        _project_id: &ProjectId,
    ) -> crate::native_sessions::discovery_state::ProjectDiscovery {
        Default::default()
    }
}
