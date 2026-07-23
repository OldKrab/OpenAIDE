use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{AgentId, ProjectId, TaskId, WorktreeId};
use openaide_app_server_protocol::snapshot::{
    NativeSessionReference, NativeSessionSummary, TaskAttentionEvent, TaskAttentionReason,
    TaskNavigationEntry, TaskNavigationSnapshot, TaskStatus as ProtocolTaskStatus, TaskSummary,
    TaskTitle, TaskTitleSource,
};

use crate::agent::registry_handle::AgentRegistryHandle;
use crate::native_sessions::catalog::NativeSessionCatalog;
use crate::projects::ProjectIdentity;
use crate::protocol::model::{TaskStatus, TaskSummary as LegacyTaskSummary};
use crate::storage::records::{
    TaskAttentionEvent as StoredTaskAttentionEvent,
    TaskAttentionReason as StoredTaskAttentionReason, TaskRecord, TaskTitle as StoredTaskTitle,
    TaskTitleSource as StoredTaskTitleSource,
};
use crate::storage::Store;

pub trait TaskNavigationSnapshotSource: Send + Sync {
    fn snapshot(
        &self,
        project_id: Option<&ProjectId>,
    ) -> Result<TaskNavigationSnapshot, ProtocolError>;
}

#[derive(Clone)]
pub struct TaskNavigationStore {
    store: Store,
    native_sessions: Option<NativeSessionCatalog>,
    agents: Option<AgentRegistryHandle>,
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
        }
    }

    pub(crate) fn with_native_sessions_and_agents(
        store: Store,
        native_sessions: NativeSessionCatalog,
        agents: AgentRegistryHandle,
    ) -> Self {
        Self {
            store,
            native_sessions: Some(native_sessions),
            agents: Some(agents),
        }
    }
}

impl TaskNavigationSnapshotSource for TaskNavigationStore {
    fn snapshot(
        &self,
        project_id: Option<&ProjectId>,
    ) -> Result<TaskNavigationSnapshot, ProtocolError> {
        let records = self.store.list_tasks().map_err(snapshot_read_error)?;
        let enabled_agents = self.agents.as_ref().map(|agents| {
            agents
                .summaries()
                .into_iter()
                .map(|agent| agent.id)
                .collect::<std::collections::HashSet<_>>()
        });
        let tasks: Vec<_> = records
            .iter()
            .cloned()
            .map(project_task_summary)
            .filter(|task| {
                enabled_agents
                    .as_ref()
                    .is_none_or(|agents| agents.contains(task.agent_id.as_str()))
            })
            .filter(|task| project_id.is_none_or(|project_id| &task.project_id == project_id))
            .collect();
        let owned = records
            .iter()
            .cloned()
            .filter_map(|record| {
                record
                    .agent_session_id
                    .map(|session_id| (record.agent_id, session_id))
            })
            .collect::<std::collections::HashSet<_>>();
        let mut entries = tasks
            .iter()
            .cloned()
            .map(|task| TaskNavigationEntry::Task {
                task: Box::new(task),
            })
            .collect::<Vec<_>>();
        if let Some(catalog) = &self.native_sessions {
            entries.extend(
                catalog
                    .entries()
                    .into_iter()
                    .filter(|entry| {
                        enabled_agents.as_ref().is_none_or(|agents| {
                            agents.contains(&entry.observation.reference.agent_id)
                        })
                    })
                    .filter(|entry| {
                        project_id.is_none_or(|project_id| entry.project_id == project_id.as_str())
                    })
                    .filter(|entry| {
                        !owned.contains(&(
                            entry.observation.reference.agent_id.clone(),
                            entry.observation.reference.session_id.clone(),
                        ))
                    })
                    .map(|entry| TaskNavigationEntry::NativeSession {
                        session: NativeSessionSummary {
                            reference: NativeSessionReference {
                                agent_id: AgentId::from(entry.observation.reference.agent_id),
                                session_id: entry.observation.reference.session_id,
                            },
                            project_id: ProjectId::from(entry.project_id),
                            workspace_root: entry.workspace_root,
                            worktree_id: None,
                            title: entry.observation.title,
                            last_activity: entry.observation.last_activity,
                        },
                    }),
            );
        }
        entries.sort_by(|left, right| {
            navigation_activity(right)
                .cmp(&navigation_activity(left))
                .then_with(|| navigation_identity(left).cmp(&navigation_identity(right)))
        });
        Ok(TaskNavigationSnapshot {
            entries,
            active_task_id: None,
            refreshing: self
                .native_sessions
                .as_ref()
                .is_some_and(NativeSessionCatalog::refreshing),
        })
    }
}

fn navigation_activity(entry: &TaskNavigationEntry) -> Option<i128> {
    match entry {
        TaskNavigationEntry::Task { task } => crate::time::activity_millis(&task.last_activity),
        TaskNavigationEntry::NativeSession { session } => session
            .last_activity
            .as_deref()
            .and_then(crate::time::activity_millis),
    }
}

fn navigation_identity(entry: &TaskNavigationEntry) -> (&str, &str, &str) {
    match entry {
        TaskNavigationEntry::Task { task } => {
            ("task", task.agent_id.as_str(), task.task_id.as_str())
        }
        TaskNavigationEntry::NativeSession { session } => (
            "nativeSession",
            session.reference.agent_id.as_str(),
            session.reference.session_id.as_str(),
        ),
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
    let title = record.title.map(project_title);
    let status = project_status_with_preparation(record.status, &record.preparation);
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
) -> ProtocolTaskStatus {
    if matches!(
        preparation,
        crate::storage::records::TaskPreparationRecord::Needed
            | crate::storage::records::TaskPreparationRecord::Preparing
    ) {
        return ProtocolTaskStatus::Preparing;
    }
    project_status(status)
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
        status: project_status(summary.status),
        updated_at: summary.updated_at,
        last_activity: summary.last_activity,
        unread: summary.unread,
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

pub(crate) fn project_status(status: TaskStatus) -> ProtocolTaskStatus {
    match status {
        TaskStatus::Starting => ProtocolTaskStatus::Starting,
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
