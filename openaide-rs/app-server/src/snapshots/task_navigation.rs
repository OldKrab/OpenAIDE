use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{AgentId, ProjectId, TaskId, WorktreeId};
use openaide_app_server_protocol::snapshot::{
    NativeSessionReference, NativeSessionSummary, TaskAttentionEvent, TaskAttentionReason,
    TaskNavigationEntry, TaskStatus as ProtocolTaskStatus, TaskSummary, TaskTitle, TaskTitleSource,
};
use openaide_app_server_protocol::task::TaskNavigationSection;

use std::collections::{HashMap, HashSet};

use crate::agent::registry_handle::AgentRegistryHandle;
use crate::native_sessions::catalog::NativeSessionCatalog;
use crate::native_sessions::discovery_state::ProjectDiscovery;
use crate::navigation::{NavigationSource, SourceGroup};
use crate::projects::{ConfiguredProjectRoots, ProjectIdentity};
use crate::protocol::model::{TaskStatus, TaskSummary as LegacyTaskSummary};
use crate::storage::records::{
    TaskAttentionEvent as StoredTaskAttentionEvent,
    TaskAttentionReason as StoredTaskAttentionReason, TaskRecord, TaskTitle as StoredTaskTitle,
    TaskTitleSource as StoredTaskTitleSource,
};
use crate::storage::Store;

#[derive(Clone)]
pub struct TaskNavigationStore {
    store: Store,
    native_sessions: Option<NativeSessionCatalog>,
    agents: Option<AgentRegistryHandle>,
    configured_projects: ConfiguredProjectRoots,
}

impl TaskNavigationStore {
    pub fn new(store: Store) -> Self {
        let native_sessions = NativeSessionCatalog::open(store.clone())
            .inspect_err(|error| {
                crate::logging::warn(
                    "native_session_catalog_open_failed",
                    serde_json::json!({ "error": error.to_string() }),
                );
            })
            .ok();
        Self {
            store,
            native_sessions,
            agents: None,
            configured_projects: ConfiguredProjectRoots::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_native_sessions(
        store: Store,
        native_sessions: NativeSessionCatalog,
    ) -> Self {
        Self {
            store,
            native_sessions: Some(native_sessions),
            agents: None,
            configured_projects: ConfiguredProjectRoots::default(),
        }
    }

    pub(crate) fn with_native_sessions_and_agents(
        store: Store,
        native_sessions: NativeSessionCatalog,
        agents: AgentRegistryHandle,
        configured_projects: ConfiguredProjectRoots,
    ) -> Self {
        Self {
            store,
            native_sessions: Some(native_sessions),
            agents: Some(agents),
            configured_projects,
        }
    }
}

impl NavigationSource for TaskNavigationStore {
    fn discovery(&self, project_id: &ProjectId) -> ProjectDiscovery {
        self.native_sessions
            .as_ref()
            .map(|catalog| catalog.discovery().project(project_id.as_str()))
            .unwrap_or_default()
    }

    fn groups(
        &self,
        section: TaskNavigationSection,
        project_ids: Option<&HashSet<ProjectId>>,
    ) -> Result<Vec<SourceGroup>, ProtocolError> {
        let records = match section {
            TaskNavigationSection::Tasks => self.store.list_tasks(),
            TaskNavigationSection::Archive => self.store.list_archived_tasks(),
        }
        .map_err(snapshot_read_error)?;
        let enabled_agents = self.agents.as_ref().map(|agents| {
            agents
                .summaries()
                .into_iter()
                .map(|agent| agent.id)
                .collect::<HashSet<_>>()
        });
        let tasks: Vec<_> = records
            .iter()
            .cloned()
            .map(|record| {
                let identity = ProjectIdentity::from_workspace_root(
                    record
                        .project_root
                        .as_deref()
                        .unwrap_or(&record.workspace_root),
                );
                (project_task_summary(record), identity.label)
            })
            // A Task nobody has written to is not history yet.
            .filter(|(task, _)| task.has_messages)
            .filter(|(task, _)| {
                enabled_agents
                    .as_ref()
                    .is_none_or(|agents| agents.contains(task.agent_id.as_str()))
            })
            .filter(|(task, _)| project_selected(project_ids, &task.project_id))
            .filter(|(task, _)| !self.configured_projects.is_removed(&task.project_id))
            .collect();
        // An archived or prepared Task still owns its Agent Native Session. Otherwise
        // archiving could incorrectly resurrect that session as an unadopted row.
        let owned = self
            .store
            .list_all_task_records()
            .map_err(snapshot_read_error)?
            .iter()
            .cloned()
            .filter_map(|record| {
                record
                    .agent_session_id
                    .map(|session_id| (record.agent_id, session_id))
            })
            .collect::<HashSet<_>>();
        let mut groups = HashMap::<ProjectId, SourceGroup>::new();
        for project in self.configured_projects.projects() {
            if project_selected(project_ids, &project.project_id) {
                groups.insert(
                    project.project_id.clone(),
                    empty_group(project.project_id, project.label),
                );
            }
        }
        for (task, project_label) in tasks {
            groups
                .entry(task.project_id.clone())
                .or_insert_with(|| empty_group(task.project_id.clone(), project_label))
                .entries
                .push(TaskNavigationEntry::Task {
                    task: Box::new(task),
                });
        }
        if let Some(catalog) = &self.native_sessions {
            for entry in catalog
                .section_entries(matches!(section, TaskNavigationSection::Archive))
                .into_iter()
                .filter(|entry| {
                    enabled_agents
                        .as_ref()
                        .is_none_or(|agents| agents.contains(&entry.observation.reference.agent_id))
                })
                .filter(|entry| project_selected_str(project_ids, &entry.project_id))
                .filter(|entry| {
                    !self
                        .configured_projects
                        .is_removed(&ProjectId::from(entry.project_id.clone()))
                })
                .filter(|entry| {
                    !owned.contains(&(
                        entry.observation.reference.agent_id.clone(),
                        entry.observation.reference.session_id.clone(),
                    ))
                })
            {
                let project_id = ProjectId::from(entry.project_id.clone());
                let project_label =
                    ProjectIdentity::from_workspace_root(&entry.workspace_root).label;
                groups
                    .entry(project_id.clone())
                    .or_insert_with(|| empty_group(project_id.clone(), project_label))
                    .entries
                    .push(TaskNavigationEntry::NativeSession {
                        session: native_session_summary(entry, project_id),
                    });
            }
        }
        Ok(groups.into_values().collect())
    }
}

fn empty_group(project_id: ProjectId, label: String) -> SourceGroup {
    SourceGroup {
        project_id,
        label,
        entries: Vec::new(),
    }
}

fn project_selected(project_ids: Option<&HashSet<ProjectId>>, project_id: &ProjectId) -> bool {
    project_ids.is_none_or(|project_ids| project_ids.contains(project_id))
}

fn project_selected_str(project_ids: Option<&HashSet<ProjectId>>, project_id: &str) -> bool {
    project_ids.is_none_or(|project_ids| {
        project_ids
            .iter()
            .any(|candidate| candidate.as_str() == project_id)
    })
}

pub(crate) fn native_session_summary(
    entry: crate::native_sessions::catalog::NativeSessionCatalogEntry,
    project_id: ProjectId,
) -> NativeSessionSummary {
    NativeSessionSummary {
        reference: NativeSessionReference {
            agent_id: AgentId::from(entry.observation.reference.agent_id),
            session_id: entry.observation.reference.session_id,
        },
        project_id,
        workspace_root: entry.workspace_root,
        worktree_id: None,
        title: entry
            .user_title
            .or(entry.observation.title)
            .or(entry.local_fallback_title),
        pinned: entry.pinned,
        last_activity: entry.observation.last_activity,
    }
}

pub(crate) fn project_task_summary(record: TaskRecord) -> TaskSummary {
    let has_messages = record.message_history_version > 0;
    project_task_summary_with_has_messages(record, has_messages)
}

pub(crate) fn project_task_summary_with_has_messages(
    record: TaskRecord,
    has_messages: bool,
) -> TaskSummary {
    let title = record.title.effective().cloned().map(project_title);
    let status = project_status_with_preparation(
        record.status,
        &record.preparation,
        record.background_only(),
    );
    let lifecycle = project_task_lifecycle(&record.lifecycle);
    let workspace_available = std::path::Path::new(&record.workspace_root).is_dir();
    TaskSummary {
        task_id: TaskId::from(record.task_id),
        project_id: ProjectIdentity::from_workspace_root(
            record
                .project_root
                .as_deref()
                .unwrap_or(&record.workspace_root),
        )
        .project_id,
        agent_id: AgentId::from(record.agent_id),
        lifecycle,
        title,
        status,
        updated_at: record.updated_at,
        last_activity: record.last_activity,
        unread: record.unread,
        pinned: record.pinned,
        attention: record.attention.map(project_attention),
        has_messages,
        worktree_id: record.worktree_id.map(WorktreeId::from),
        workspace_available,
    }
}

fn project_title(title: StoredTaskTitle) -> TaskTitle {
    TaskTitle {
        value: title.value().to_string(),
        source: match title.source() {
            StoredTaskTitleSource::Prompt => TaskTitleSource::Prompt,
            StoredTaskTitleSource::Agent => TaskTitleSource::Agent,
            StoredTaskTitleSource::User => TaskTitleSource::User,
        },
    }
}

pub(crate) fn project_status_with_preparation(
    status: TaskStatus,
    preparation: &crate::storage::records::TaskPreparationRecord,
    background_only: bool,
) -> ProtocolTaskStatus {
    if matches!(
        preparation,
        crate::storage::records::TaskPreparationRecord::Needed
            | crate::storage::records::TaskPreparationRecord::Preparing
    ) {
        return ProtocolTaskStatus::Preparing;
    }
    project_status(status, background_only)
}

pub(crate) fn project_legacy_task_summary(
    summary: LegacyTaskSummary,
    has_messages: bool,
    lifecycle: openaide_app_server_protocol::snapshot::TaskLifecycle,
) -> TaskSummary {
    let workspace_available = std::path::Path::new(&summary.workspace_root).is_dir();
    TaskSummary {
        task_id: TaskId::from(summary.task_id),
        project_id: ProjectIdentity::from_workspace_root(
            summary
                .project_root
                .as_deref()
                .unwrap_or(&summary.workspace_root),
        )
        .project_id,
        agent_id: AgentId::from(summary.agent_id),
        lifecycle,
        title: summary.title.map(project_title),
        status: project_status(
            summary.status,
            summary
                .background_work
                .as_ref()
                .is_some_and(|work| work.held),
        ),
        updated_at: summary.updated_at,
        last_activity: summary.last_activity,
        unread: summary.unread,
        pinned: summary.pinned,
        attention: summary.attention.map(project_attention),
        has_messages,
        worktree_id: summary.worktree_id.map(WorktreeId::from),
        workspace_available,
    }
}

pub(crate) fn project_task_lifecycle(
    lifecycle: &crate::storage::records::TaskLifecycle,
) -> openaide_app_server_protocol::snapshot::TaskLifecycle {
    match lifecycle {
        crate::storage::records::TaskLifecycle::Prepared { .. } => {
            openaide_app_server_protocol::snapshot::TaskLifecycle::Prepared
        }
        crate::storage::records::TaskLifecycle::Open => {
            openaide_app_server_protocol::snapshot::TaskLifecycle::Open
        }
        crate::storage::records::TaskLifecycle::Archived => {
            openaide_app_server_protocol::snapshot::TaskLifecycle::Archived
        }
    }
}

fn project_attention(attention: StoredTaskAttentionEvent) -> TaskAttentionEvent {
    TaskAttentionEvent {
        event_id: attention.event_id,
        reason: match attention.reason {
            StoredTaskAttentionReason::Finished => TaskAttentionReason::Finished,
            StoredTaskAttentionReason::NeedsPermission => TaskAttentionReason::NeedsPermission,
            StoredTaskAttentionReason::NeedsAnswer => TaskAttentionReason::NeedsAnswer,
            StoredTaskAttentionReason::Stopped => TaskAttentionReason::Stopped,
            StoredTaskAttentionReason::Failed => TaskAttentionReason::Failed,
        },
        occurred_at: attention.occurred_at,
    }
}

/// `background_only` is the single place the display-only `Background` status
/// is derived; the stored status of such a Task stays `Active`.
pub(crate) fn project_status(status: TaskStatus, background_only: bool) -> ProtocolTaskStatus {
    match status {
        TaskStatus::Starting => ProtocolTaskStatus::Starting,
        TaskStatus::Active if background_only => ProtocolTaskStatus::Background,
        TaskStatus::Active => ProtocolTaskStatus::Running,
        TaskStatus::Stopping => ProtocolTaskStatus::Stopping,
        TaskStatus::Inactive => ProtocolTaskStatus::Idle,
        TaskStatus::Failed => ProtocolTaskStatus::Failed,
        TaskStatus::Completed => ProtocolTaskStatus::Completed,
        TaskStatus::Waiting => ProtocolTaskStatus::Waiting,
    }
}

pub(crate) fn snapshot_read_error(error: impl std::fmt::Display) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Internal,
        message: format!("Failed to read task navigation snapshot: {error}"),
        recoverable: true,
        target: None,
    }
}

#[cfg(test)]
#[path = "task_navigation_tests.rs"]
mod tests;
