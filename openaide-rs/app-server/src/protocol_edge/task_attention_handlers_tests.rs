use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, ProjectId, TaskId, TaskListCursor};
use openaide_app_server_protocol::snapshot::{
    TaskAttentionEvent, TaskAttentionReason, TaskLifecycle, TaskSnapshot, TaskStatus, TaskSummary,
};
use openaide_app_server_protocol::task::{TaskListLifecycle, ToolDetailSnapshot};

use super::current_task_attention;
use crate::snapshots::{TaskListSnapshot, TaskSnapshotSource};

/// Open Tasks a test replaces to stand in for committed Task changes.
#[derive(Default)]
pub(crate) struct OpenTasks {
    tasks: Mutex<Vec<TaskSummary>>,
}

impl OpenTasks {
    pub(crate) fn new(tasks: Vec<TaskSummary>) -> Arc<Self> {
        Arc::new(Self {
            tasks: Mutex::new(tasks),
        })
    }

    pub(crate) fn replace(&self, tasks: Vec<TaskSummary>) {
        *self.tasks.lock().unwrap() = tasks;
    }
}

fn not_found() -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotFound,
        message: "not part of this fixture".to_string(),
        recoverable: false,
        target: None,
    }
}

impl TaskSnapshotSource for OpenTasks {
    fn list(
        &self,
        lifecycle: TaskListLifecycle,
        _project_id: Option<&ProjectId>,
        _cursor: Option<&TaskListCursor>,
    ) -> Result<TaskListSnapshot, ProtocolError> {
        let tasks = match lifecycle {
            TaskListLifecycle::Open => self.tasks.lock().unwrap().clone(),
            TaskListLifecycle::Archived => Vec::new(),
        };
        Ok(TaskListSnapshot {
            tasks,
            revision: 0,
            next_cursor: None,
        })
    }

    fn open_internal(&self, _task_id: &TaskId) -> Result<TaskSnapshot, ProtocolError> {
        Err(not_found())
    }

    fn open_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _task_id: &TaskId,
    ) -> Result<TaskSnapshot, ProtocolError> {
        Err(not_found())
    }

    fn tool_detail_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _task_id: &TaskId,
        _artifact_id: &str,
    ) -> Result<ToolDetailSnapshot, ProtocolError> {
        Err(not_found())
    }
}

/// An open Task; `event_id` gives it a current Task Attention Event.
pub(crate) fn task(task_id: &str, event_id: Option<&str>, status: TaskStatus) -> TaskSummary {
    TaskSummary {
        task_id: task_id.into(),
        project_id: "project-1".into(),
        agent_id: "codex".into(),
        lifecycle: TaskLifecycle::Open,
        title: None,
        status,
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        last_activity: "2026-01-01T00:00:00Z".to_string(),
        unread: false,
        pinned: false,
        attention: event_id.map(|event_id| TaskAttentionEvent {
            event_id: event_id.to_string(),
            reason: TaskAttentionReason::Finished,
            occurred_at: "1767225600000".to_string(),
        }),
        has_messages: true,
        worktree_id: None,
        workspace_available: true,
    }
}

#[test]
fn lists_only_tasks_with_a_current_attention_event() {
    let tasks = OpenTasks::new(vec![
        task("task-b", Some("event-2"), TaskStatus::Idle),
        task("task-quiet", None, TaskStatus::Running),
        task("task-a", Some("event-1"), TaskStatus::Idle),
    ]);

    let result = current_task_attention(tasks.as_ref()).unwrap();

    let ids: Vec<_> = result
        .tasks
        .iter()
        .map(|task| task.task_id.as_str())
        .collect();
    assert_eq!(ids, ["task-a", "task-b"]);
    assert_eq!(result.cursor.len(), 16);
}

#[test]
fn cursor_follows_event_identity_and_ignores_other_task_changes() {
    let tasks = OpenTasks::new(vec![task("task-1", Some("event-1"), TaskStatus::Idle)]);
    let first = current_task_attention(tasks.as_ref()).unwrap().cursor;

    tasks.replace(vec![
        task("task-1", Some("event-1"), TaskStatus::Running),
        task("task-2", None, TaskStatus::Running),
    ]);
    assert_eq!(
        current_task_attention(tasks.as_ref()).unwrap().cursor,
        first
    );

    tasks.replace(vec![task("task-1", Some("event-2"), TaskStatus::Idle)]);
    let replaced = current_task_attention(tasks.as_ref()).unwrap().cursor;
    assert_ne!(replaced, first);

    tasks.replace(Vec::new());
    let cleared = current_task_attention(tasks.as_ref()).unwrap().cursor;
    assert_ne!(cleared, first);
    assert_ne!(cleared, replaced);
}
