//! Row model, the one comparator, the live rule, and group ordering.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::navigation::NavigationRowRef;
use openaide_app_server_protocol::snapshot::{
    TaskNavigationEntry, TaskStatus as ProtocolTaskStatus,
};
use openaide_app_server_protocol::task::TaskNavigationSection;

use crate::native_sessions::discovery_state::ProjectDiscovery;

/// Unsorted rows of one Project that pass the membership filters.
pub(crate) struct SourceGroup {
    pub(crate) project_id: ProjectId,
    pub(crate) label: String,
    pub(crate) entries: Vec<TaskNavigationEntry>,
}

/// Where Navigation reads membership-filtered rows and discovery progress.
pub(crate) trait NavigationSource: Send + Sync {
    /// Groups for `projects`, or every Project when `None`. Membership filters are applied
    /// here: enabled Agent, removed Projects, and Tasks without messages. Configured Projects
    /// appear even when empty.
    fn groups(
        &self,
        section: TaskNavigationSection,
        projects: Option<&HashSet<ProjectId>>,
    ) -> Result<Vec<SourceGroup>, ProtocolError>;

    fn discovery(&self, project_id: &ProjectId) -> ProjectDiscovery;
}

/// One sorted row. `hash` fingerprints the body so a view can tell what it last sent.
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) entry: TaskNavigationEntry,
    pub(crate) live_by_state: bool,
    pub(crate) hash: u64,
    sort: SortKey,
}

struct SortKey {
    pinned: bool,
    active: bool,
    activity: Option<i128>,
    identity: (&'static str, String, String),
}

pub(crate) struct ProjectRows {
    pub(crate) label: String,
    pub(crate) rows: Vec<Row>,
    /// Whether any Task is running or has Background Work, which ranks the whole group first.
    pub(crate) has_in_progress: bool,
    /// The group's activity: its running Tasks' newest clock when it has any (listing
    /// timestamps are heartbeats and must not leapfrog a live Project), else its newest row.
    pub(crate) activity: Option<i128>,
}

pub(crate) fn row_id(entry: &TaskNavigationEntry) -> String {
    match entry {
        TaskNavigationEntry::Task { task } => format!("task:{}", task.task_id.as_str()),
        TaskNavigationEntry::NativeSession { session } => format!(
            "session:{}:{}",
            session.reference.agent_id.as_str(),
            session.reference.session_id
        ),
    }
}

pub(crate) fn row_ref_id(row: &NavigationRowRef) -> String {
    match row {
        NavigationRowRef::Task { task_id } => format!("task:{}", task_id.as_str()),
        NavigationRowRef::NativeSession {
            agent_id,
            session_id,
        } => format!("session:{}:{session_id}", agent_id.as_str()),
    }
}

impl ProjectRows {
    pub(crate) fn build(group: SourceGroup) -> Self {
        let mut rows: Vec<Row> = group.entries.into_iter().map(Row::new).collect();
        rows.sort_by(|left, right| left.sort.compare(&right.sort));
        let has_in_progress = rows.iter().any(|row| row.sort.active);
        let activity = rows
            .iter()
            .filter(|row| !has_in_progress || row.sort.active)
            .filter_map(|row| row.sort.activity)
            .max();
        Self {
            label: group.label,
            rows,
            has_in_progress,
            activity,
        }
    }
}

impl Row {
    fn new(entry: TaskNavigationEntry) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // Serialization of a plain data value cannot fail; an empty body would only force a resend.
        serde_json::to_vec(&entry)
            .unwrap_or_default()
            .hash(&mut hasher);
        Self {
            id: row_id(&entry),
            live_by_state: live_by_state(&entry),
            hash: hasher.finish(),
            sort: SortKey::of(&entry),
            entry,
        }
    }
}

impl SortKey {
    fn of(entry: &TaskNavigationEntry) -> Self {
        match entry {
            TaskNavigationEntry::Task { task } => Self {
                pinned: task.pinned,
                active: matches!(
                    task.status,
                    ProtocolTaskStatus::Running | ProtocolTaskStatus::Background
                ),
                activity: crate::time::activity_millis(&task.last_activity),
                identity: (
                    "task",
                    task.agent_id.as_str().to_string(),
                    task.task_id.as_str().to_string(),
                ),
            },
            TaskNavigationEntry::NativeSession { session } => Self {
                pinned: session.pinned,
                active: false,
                activity: session
                    .last_activity
                    .as_deref()
                    .and_then(crate::time::activity_millis),
                identity: (
                    "nativeSession",
                    session.reference.agent_id.as_str().to_string(),
                    session.reference.session_id.clone(),
                ),
            },
        }
    }

    /// Pinned, then running, then newest activity, then identity. The single comparator for
    /// every client, Tasks and Native Sessions alike.
    fn compare(&self, other: &Self) -> std::cmp::Ordering {
        other
            .pinned
            .cmp(&self.pinned)
            .then_with(|| other.active.cmp(&self.active))
            .then_with(|| other.activity.cmp(&self.activity))
            .then_with(|| self.identity.cmp(&other.identity))
    }
}

/// Rows the user may need to act on now, which the history limit never hides: pinned rows,
/// unread, running, background, stopping or waiting Tasks, and Tasks with an attention event.
pub(crate) fn live_by_state(entry: &TaskNavigationEntry) -> bool {
    match entry {
        TaskNavigationEntry::Task { task } => {
            task.pinned
                || task.unread
                || task.attention.is_some()
                || matches!(
                    task.status,
                    ProtocolTaskStatus::Running
                        | ProtocolTaskStatus::Background
                        | ProtocolTaskStatus::Stopping
                        | ProtocolTaskStatus::Waiting
                )
        }
        TaskNavigationEntry::NativeSession { session } => session.pinned,
    }
}

/// Groups with running work first, then by activity (newest first), then label, then id.
pub(crate) fn compare_groups(
    left: (&ProjectId, &ProjectRows),
    right: (&ProjectId, &ProjectRows),
) -> std::cmp::Ordering {
    right
        .1
        .has_in_progress
        .cmp(&left.1.has_in_progress)
        .then_with(|| right.1.activity.cmp(&left.1.activity))
        .then_with(|| left.1.label.cmp(&right.1.label))
        .then_with(|| left.0.cmp(right.0))
}
