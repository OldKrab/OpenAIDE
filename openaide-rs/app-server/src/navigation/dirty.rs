use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::ids::ProjectId;

#[derive(Default)]
struct Marks {
    projects: HashSet<ProjectId>,
    all: bool,
}

/// What changed since the last flush. Committers mark it synchronously, so by the time a
/// handler replies the owner already knows every Project that handler touched.
#[derive(Clone, Default)]
pub(crate) struct NavigationDirty {
    marks: Arc<Mutex<Marks>>,
}

/// The marks one flush consumes.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct DirtyMarks {
    pub(crate) projects: HashSet<ProjectId>,
    /// Projects or Agents changed: every Project is rebuilt.
    pub(crate) all: bool,
}

impl DirtyMarks {
    pub(crate) fn is_empty(&self) -> bool {
        !self.all && self.projects.is_empty()
    }
}

impl NavigationDirty {
    /// Returns whether the set was empty before, so the caller wakes the flusher once.
    pub(crate) fn mark_project(&self, project_id: ProjectId) -> bool {
        let mut marks = self.lock();
        let was_empty = marks.projects.is_empty() && !marks.all;
        marks.projects.insert(project_id);
        was_empty
    }

    pub(crate) fn mark_all(&self) -> bool {
        let mut marks = self.lock();
        let was_empty = marks.projects.is_empty() && !marks.all;
        marks.all = true;
        was_empty
    }

    pub(crate) fn is_dirty(&self) -> bool {
        let marks = self.lock();
        marks.all || !marks.projects.is_empty()
    }

    pub(crate) fn take(&self) -> DirtyMarks {
        let mut marks = self.lock();
        DirtyMarks {
            projects: std::mem::take(&mut marks.projects),
            all: std::mem::take(&mut marks.all),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Marks> {
        self.marks.lock().expect("navigation dirty marks poisoned")
    }
}
