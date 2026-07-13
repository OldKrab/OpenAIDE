use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{AgentId, ProjectId, TaskId};
use openaide_app_server_protocol::snapshot::{
    TaskNavigationSnapshot, TaskStatus as ProtocolTaskStatus, TaskSummary, TaskTitle,
    TaskTitleSource,
};

use crate::projects::ProjectIdentity;
use crate::protocol::model::{TaskStatus, TaskSummary as LegacyTaskSummary};
use crate::storage::records::{
    TaskRecord, TaskTitle as StoredTaskTitle, TaskTitleSource as StoredTaskTitleSource,
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
}

impl TaskNavigationStore {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl TaskNavigationSnapshotSource for TaskNavigationStore {
    fn snapshot(
        &self,
        project_id: Option<&ProjectId>,
    ) -> Result<TaskNavigationSnapshot, ProtocolError> {
        let tasks = self
            .store
            .list_tasks()
            .map_err(snapshot_read_error)?
            .into_iter()
            .map(project_task_summary)
            .filter(|task| project_id.is_none_or(|project_id| &task.project_id == project_id))
            .collect();
        Ok(TaskNavigationSnapshot {
            tasks,
            active_task_id: None,
        })
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
    TaskSummary {
        task_id: TaskId::from(record.task_id),
        project_id: ProjectIdentity::from_workspace_root(&record.workspace_root).project_id,
        agent_id: AgentId::from(record.agent_id),
        title,
        status,
        updated_at: record.updated_at,
        last_activity: record.last_activity,
        unread: record.unread,
        has_messages,
    }
}

fn project_title(title: StoredTaskTitle) -> TaskTitle {
    TaskTitle {
        value: title.value().to_string(),
        source: match title.source() {
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
) -> TaskSummary {
    TaskSummary {
        task_id: TaskId::from(summary.task_id),
        project_id: ProjectIdentity::from_workspace_root(&summary.workspace_root).project_id,
        agent_id: AgentId::from(summary.agent_id),
        title: summary.title.map(project_title),
        status: project_status(summary.status),
        updated_at: summary.updated_at,
        last_activity: summary.last_activity,
        unread: summary.unread,
        has_messages,
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
