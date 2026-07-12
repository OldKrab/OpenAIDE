use serde::Serialize;

use openaide_app_server_protocol::diagnostics::{
    ActiveTaskDiagnosticsResult, DiagnosticsRedaction, RuntimeDiagnosticsResult,
    RuntimeDiagnosticsStatus, TaskDiagnosticsResult,
};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{AgentId, TaskId};
use openaide_app_server_protocol::methods::CLIENT_METHODS;
use openaide_app_server_protocol::snapshot::TaskStatus as ProtocolTaskStatus;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus;
use crate::storage::records::TaskRecord;
use crate::storage::Store;
use crate::tasks::query_store::TaskReadStore;

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticNotice {
    pub component: &'static str,
    pub severity: &'static str,
    pub message: String,
}

impl DiagnosticNotice {
    pub fn redacted(
        component: &'static str,
        severity: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            component,
            severity,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeDiagnostics {
    pub status: &'static str,
    pub version: String,
    pub method_count: usize,
    pub tasks: TaskDiagnostics,
    pub redaction: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDiagnostics {
    pub visible_count: usize,
    pub total_count: usize,
    pub active_count: usize,
    pub active_tasks: Vec<ActiveTaskDiagnostics>,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveTaskDiagnostics {
    pub task_id: String,
    pub agent_id: String,
    pub status: TaskStatus,
    pub updated_at: String,
    pub last_activity: String,
    pub active_turn_id: Option<String>,
    pub has_agent_session: bool,
}

impl TaskDiagnostics {
    pub fn from_records(visible_tasks: &[TaskRecord], total_count: usize, revision: u64) -> Self {
        let active_tasks = visible_tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Active || task.active_turn_id.is_some())
            .map(active_task_diagnostics)
            .collect::<Vec<_>>();
        Self {
            visible_count: visible_tasks.len(),
            total_count,
            active_count: active_tasks.len(),
            active_tasks,
            revision,
        }
    }
}

fn active_task_diagnostics(task: &TaskRecord) -> ActiveTaskDiagnostics {
    ActiveTaskDiagnostics {
        task_id: task.task_id.clone(),
        agent_id: task.agent_id.clone(),
        status: task.status,
        updated_at: task.updated_at.clone(),
        last_activity: task.last_activity.clone(),
        active_turn_id: task.active_turn_id.clone(),
        has_agent_session: task.agent_session_id.is_some(),
    }
}

pub(crate) trait RuntimeDiagnosticsWorkflow: Send + Sync {
    fn runtime_diagnostics(&self) -> Result<RuntimeDiagnosticsResult, ProtocolError>;
}

#[derive(Clone)]
pub(crate) struct RuntimeDiagnosticsService {
    store: Store,
}

impl RuntimeDiagnosticsService {
    pub(crate) fn new(store: Store) -> Self {
        Self { store }
    }
}

impl RuntimeDiagnosticsWorkflow for RuntimeDiagnosticsService {
    fn runtime_diagnostics(&self) -> Result<RuntimeDiagnosticsResult, ProtocolError> {
        let revision = self.store.max_task_revision().map_err(protocol_error)?;
        let diagnostics = TaskReadStore::new(self.store.clone())
            .diagnostics(revision)
            .map_err(protocol_error)?;
        Ok(RuntimeDiagnosticsResult {
            status: RuntimeDiagnosticsStatus::Ready,
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            method_count: CLIENT_METHODS.len(),
            tasks: protocol_task_diagnostics(diagnostics),
            redaction: DiagnosticsRedaction::PromptTextFileContentsTerminalOutputAndSecretsRemoved,
        })
    }
}

fn protocol_task_diagnostics(diagnostics: TaskDiagnostics) -> TaskDiagnosticsResult {
    TaskDiagnosticsResult {
        visible_count: diagnostics.visible_count,
        total_count: diagnostics.total_count,
        active_count: diagnostics.active_count,
        active_tasks: diagnostics
            .active_tasks
            .into_iter()
            .map(protocol_active_task_diagnostics)
            .collect(),
        revision: diagnostics.revision,
    }
}

fn protocol_active_task_diagnostics(
    diagnostics: ActiveTaskDiagnostics,
) -> ActiveTaskDiagnosticsResult {
    ActiveTaskDiagnosticsResult {
        task_id: TaskId::from(diagnostics.task_id),
        agent_id: AgentId::from(diagnostics.agent_id),
        status: protocol_task_status(diagnostics.status),
        updated_at: diagnostics.updated_at,
        last_activity: diagnostics.last_activity,
        active_turn_id: diagnostics.active_turn_id,
        has_agent_session: diagnostics.has_agent_session,
    }
}

fn protocol_task_status(status: TaskStatus) -> ProtocolTaskStatus {
    match status {
        TaskStatus::Starting => ProtocolTaskStatus::Starting,
        TaskStatus::Active => ProtocolTaskStatus::Running,
        TaskStatus::Inactive => ProtocolTaskStatus::Idle,
        TaskStatus::Failed => ProtocolTaskStatus::Failed,
        TaskStatus::Completed => ProtocolTaskStatus::Completed,
        TaskStatus::Blocked => ProtocolTaskStatus::Blocked,
    }
}

fn protocol_error(error: RuntimeError) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Internal,
        message: error.to_string(),
        recoverable: true,
        target: None,
    }
}
