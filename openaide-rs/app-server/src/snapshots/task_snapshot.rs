use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ProjectId, TaskId, TaskListCursor};
use openaide_app_server_protocol::snapshot::{ChatSnapshot, TaskSnapshot, TaskSummary};

use crate::chat_history::ChatHistoryPolicy;
use crate::protocol::model::TaskSnapshot as StoredTaskSnapshot;
use crate::storage::Store;
use crate::tasks::snapshot::build_snapshot;

pub(crate) use chat_projection::project_chat_item;
use readiness::{
    agent_commands_snapshot, agent_config_snapshot, preparation_snapshot, send_capability_for_task,
};

use super::task_navigation::{
    project_legacy_task_summary, project_status_with_preparation, project_task_summary,
    snapshot_read_error,
};

mod chat_projection;
mod readiness;

pub trait TaskSnapshotSource: Send + Sync {
    fn list(
        &self,
        archived: bool,
        project_id: Option<&ProjectId>,
        cursor: Option<&TaskListCursor>,
    ) -> Result<TaskListSnapshot, ProtocolError>;

    fn open(&self, task_id: &TaskId) -> Result<TaskSnapshot, ProtocolError>;
}

#[derive(Debug, Clone)]
pub struct TaskListSnapshot {
    pub tasks: Vec<TaskSummary>,
    pub revision: u64,
    pub next_cursor: Option<TaskListCursor>,
}

#[derive(Clone)]
pub struct TaskSnapshotStore {
    store: Store,
    tail_limit: usize,
}

impl TaskSnapshotStore {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            tail_limit: ChatHistoryPolicy::default().task_snapshot_tail_limit(),
        }
    }
}

impl TaskSnapshotSource for TaskSnapshotStore {
    fn list(
        &self,
        archived: bool,
        project_id: Option<&ProjectId>,
        cursor: Option<&TaskListCursor>,
    ) -> Result<TaskListSnapshot, ProtocolError> {
        if cursor.is_some() {
            return Err(unsupported_cursor_error());
        }
        let tasks = self
            .store
            .list_tasks_strict_for_archive(archived)
            .map_err(snapshot_read_error)?
            .into_iter()
            .map(project_task_summary)
            .filter(|task| project_id.is_none_or(|project_id| &task.project_id == project_id))
            .collect();
        let revision = self
            .store
            .max_task_revision()
            .map_err(snapshot_read_error)?;
        Ok(TaskListSnapshot {
            tasks,
            revision,
            next_cursor: None,
        })
    }

    fn open(&self, task_id: &TaskId) -> Result<TaskSnapshot, ProtocolError> {
        let task = self
            .store
            .read_task(task_id.as_str())
            .map_err(task_snapshot_error)?;
        if task.tombstoned {
            return Err(ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: format!("task not found: {}", task_id.as_str()),
                recoverable: false,
                target: None,
            });
        }
        let snapshot = build_snapshot(&self.store, task_id.as_str(), self.tail_limit)
            .map_err(task_snapshot_error)?;
        project_stored_task_snapshot(snapshot)
    }
}

trait TaskSnapshotStoreArchiveList {
    fn list_tasks_strict_for_archive(
        &self,
        archived: bool,
    ) -> Result<Vec<crate::storage::records::TaskRecord>, crate::protocol::errors::RuntimeError>;
}

impl TaskSnapshotStoreArchiveList for Store {
    fn list_tasks_strict_for_archive(
        &self,
        archived: bool,
    ) -> Result<Vec<crate::storage::records::TaskRecord>, crate::protocol::errors::RuntimeError>
    {
        if archived {
            self.list_archived_tasks_strict()
        } else {
            self.list_tasks_strict()
        }
    }
}

pub(crate) fn project_stored_task_snapshot(
    snapshot: StoredTaskSnapshot,
) -> Result<TaskSnapshot, ProtocolError> {
    let send_capability = send_capability_for_task(snapshot.task.status, &snapshot.preparation);
    let agent_config = agent_config_snapshot(&snapshot);
    let agent_commands = agent_commands_snapshot(&snapshot);
    let projected_status =
        project_status_with_preparation(snapshot.task.status, &snapshot.preparation);
    let mut task = project_legacy_task_summary(snapshot.task, snapshot.chat.total_count > 0);
    task.status = projected_status;
    Ok(TaskSnapshot {
        task,
        revision: snapshot.revision,
        preparation: preparation_snapshot(&snapshot.preparation),
        agent_config,
        agent_commands,
        send_capability,
        chat: ChatSnapshot {
            items: snapshot.chat.items.iter().map(project_chat_item).collect(),
            has_more_before: snapshot.chat.has_before,
            has_messages: snapshot.chat.total_count > 0,
            start_cursor: snapshot.chat.start_cursor.map(Into::into),
            end_cursor: snapshot.chat.end_cursor.map(Into::into),
        },
        history_sync: Default::default(),
        pending_requests: Vec::new(),
        recovery: None,
    })
}

fn unsupported_cursor_error() -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::CapabilityUnavailable,
        message: "Task list pagination is not available in this API slice".to_string(),
        recoverable: true,
        target: None,
    }
}

fn task_snapshot_error(error: impl std::fmt::Display) -> ProtocolError {
    let message = error.to_string();
    let code = if message.starts_with("task not found") {
        ProtocolErrorCode::NotFound
    } else {
        ProtocolErrorCode::Internal
    };
    ProtocolError {
        code,
        message,
        recoverable: code != ProtocolErrorCode::NotFound,
        target: None,
    }
}

#[cfg(test)]
mod tests;
