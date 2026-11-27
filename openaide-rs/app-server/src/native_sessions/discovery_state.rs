//! Per-Project Native Session discovery state, shared by the discovery workers that write it
//! and the Navigation owner that reads it (design 2.2).
//!
//! The Navigation owner also writes the demand board: how many history rows the open Tasks
//! views want per Project. Discovery reads it at the start of each context, so its depth is a
//! count target derived from the views and a Project nobody shows is never listed.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// What discovery knows about one Project, as Navigation reports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ProjectDiscovery {
    /// Discovery stopped before exhausting an Agent's history, so more rows may exist.
    pub(crate) undiscovered: bool,
    /// A discovery run for this Project is in flight.
    pub(crate) loading: bool,
    /// The last discovery run for this Project failed.
    pub(crate) failed: bool,
}

#[derive(Default)]
struct Inner {
    undiscovered: HashSet<String>,
    loading: HashSet<String>,
    failed: HashSet<String>,
    demand: HashMap<String, usize>,
}

#[derive(Clone, Default)]
pub(crate) struct DiscoveryState {
    inner: Arc<Mutex<Inner>>,
}

impl DiscoveryState {
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .expect("native session discovery state poisoned")
    }

    pub(crate) fn project(&self, project_id: &str) -> ProjectDiscovery {
        let inner = self.inner();
        ProjectDiscovery {
            undiscovered: inner.undiscovered.contains(project_id),
            loading: inner.loading.contains(project_id),
            failed: inner.failed.contains(project_id),
        }
    }

    /// Each setter returns whether the value changed, so callers mark Navigation dirty only then.
    pub(crate) fn set_undiscovered(&self, project_id: &str, undiscovered: bool) -> bool {
        set_member(&mut self.inner().undiscovered, project_id, undiscovered)
    }

    pub(crate) fn set_loading(&self, project_id: &str, loading: bool) -> bool {
        set_member(&mut self.inner().loading, project_id, loading)
    }

    pub(crate) fn set_failed(&self, project_id: &str, failed: bool) -> bool {
        set_member(&mut self.inner().failed, project_id, failed)
    }

    /// Replaces the demand board. Returns the Projects whose target grew, which discovery
    /// must now deepen.
    pub(crate) fn replace_demand(&self, demand: HashMap<String, usize>) -> Vec<(String, usize)> {
        let mut inner = self.inner();
        let grown = demand
            .iter()
            .filter(|(project, target)| {
                inner
                    .demand
                    .get(*project)
                    .is_none_or(|previous| previous < *target)
            })
            .map(|(project, target)| (project.clone(), *target))
            .collect();
        inner.demand = demand;
        grown
    }

    /// Every Project a Tasks view currently shows.
    pub(crate) fn demanded_projects(&self) -> Vec<String> {
        self.inner().demand.keys().cloned().collect()
    }

    /// The row target for a Project, or `None` when no Tasks view shows it.
    pub(crate) fn target(&self, project_id: &str) -> Option<usize> {
        self.inner().demand.get(project_id).copied()
    }
}

fn set_member(set: &mut HashSet<String>, project_id: &str, present: bool) -> bool {
    if present {
        set.insert(project_id.to_string())
    } else {
        set.remove(project_id)
    }
}
