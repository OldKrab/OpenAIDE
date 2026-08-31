use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::agent::TurnCancellation;
use crate::protocol::errors::RuntimeError;
use crate::storage::records::{TaskLifecycle, TaskPreparationRecord, TaskRecord};

use super::NativeSessionService;

impl NativeSessionService {
    /// Cancels slow Agent work that no longer belongs to a leased Prepared Task.
    pub(crate) fn cancel_task_preparation(&self, task_id: &str, reason: &'static str) -> bool {
        let cancellation = self
            .preparation_cancellations
            .lock()
            .expect("Task preparation cancellation registry poisoned")
            .get(task_id)
            .cloned();
        let cancelled = cancellation.is_some();
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
        }
        crate::logging::info(
            "native_session_prepare_cancel_requested",
            serde_json::json!({
                "task_id": task_id,
                "reason": reason,
                "had_active_preparation": cancelled,
            }),
        );
        cancelled
    }

    /// Acquires and binds the empty New Task's Native Session before Composer becomes sendable.
    pub(crate) fn prepare_task(&self, task: &TaskRecord) -> Result<(), RuntimeError> {
        let cancellation = Arc::new(TurnCancellation::new());
        let _registration = TaskPreparationCancellationRegistration::register(
            self.preparation_cancellations.clone(),
            task.task_id.clone(),
            cancellation.clone(),
        )?;
        let started_at = Instant::now();
        crate::logging::info(
            "native_session_prepare_started",
            serde_json::json!({
                "task_id": task.task_id,
                "agent_id": task.agent_id,
                "has_bound_session": task.agent_session_id.is_some(),
            }),
        );
        let current = match self.store.read_task(&task.task_id) {
            Ok(current) => current,
            Err(RuntimeError::TaskNotFound(_)) => {
                log_prepare_cancelled(task, started_at, "task_no_longer_exists");
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if current.tombstoned
            || !matches!(
                current.lifecycle,
                TaskLifecycle::Prepared { lease: Some(_) }
            )
            || !matches!(current.preparation, TaskPreparationRecord::Preparing)
        {
            log_prepare_cancelled(task, started_at, "task_no_longer_leased");
            return Ok(());
        }
        let result = self.prepare_task_inner(task, cancellation.as_ref().clone());
        if cancellation.is_cancelled() {
            log_prepare_cancelled(task, started_at, "cancellation_requested");
            return Ok(());
        }
        match &result {
            Ok(()) => crate::logging::info(
                "native_session_prepare_completed",
                serde_json::json!({
                    "task_id": task.task_id,
                    "agent_id": task.agent_id,
                    "duration_ms": started_at.elapsed().as_millis(),
                }),
            ),
            Err(_) => crate::logging::warn(
                "native_session_prepare_failed",
                serde_json::json!({
                    "task_id": task.task_id,
                    "agent_id": task.agent_id,
                    "duration_ms": started_at.elapsed().as_millis(),
                    "error_kind": "runtime_error",
                }),
            ),
        }
        result
    }
}

fn log_prepare_cancelled(task: &TaskRecord, started_at: Instant, reason: &'static str) {
    crate::logging::info(
        "native_session_prepare_cancelled",
        serde_json::json!({
            "task_id": task.task_id,
            "agent_id": task.agent_id,
            "duration_ms": started_at.elapsed().as_millis(),
            "reason": reason,
        }),
    );
}

struct TaskPreparationCancellationRegistration {
    task_id: String,
    cancellation: Arc<TurnCancellation>,
    registry: Arc<Mutex<HashMap<String, Arc<TurnCancellation>>>>,
}

impl TaskPreparationCancellationRegistration {
    fn register(
        registry: Arc<Mutex<HashMap<String, Arc<TurnCancellation>>>>,
        task_id: String,
        cancellation: Arc<TurnCancellation>,
    ) -> Result<Self, RuntimeError> {
        let mut registrations = registry
            .lock()
            .expect("Task preparation cancellation registry poisoned");
        if registrations.contains_key(&task_id) {
            return Err(RuntimeError::Conflict(
                "Task Agent preparation is already running".to_string(),
            ));
        }
        registrations.insert(task_id.clone(), cancellation.clone());
        drop(registrations);
        Ok(Self {
            task_id,
            cancellation,
            registry,
        })
    }
}

impl Drop for TaskPreparationCancellationRegistration {
    fn drop(&mut self) {
        let mut registrations = self
            .registry
            .lock()
            .expect("Task preparation cancellation registry poisoned");
        if registrations
            .get(&self.task_id)
            .is_some_and(|current| Arc::ptr_eq(current, &self.cancellation))
        {
            registrations.remove(&self.task_id);
        }
    }
}
