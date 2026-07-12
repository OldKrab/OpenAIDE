use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};

use crate::agent::{AgentSessionResume, AgentSessionStart, ConfigOptionPolicy, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::tasks::task_start_transaction::TaskSessionStartGuard;
use crate::time::now_string;

use super::{internal_error, TaskProductApi};

impl TaskProductApi {
    pub(super) fn spawn_task_preparation(&self, task: TaskRecord) {
        let api = self.clone();
        std::thread::spawn(move || {
            if let Err(error) = api.prepare_task_native_session(&task) {
                let _ = api.persist_preparation_failure(&task.task_id, &error);
            }
        });
    }

    fn prepare_task_native_session(&self, task: &TaskRecord) -> Result<(), RuntimeError> {
        let cancellation = TurnCancellation::new();
        let start = || {
            self.agent_gateway.start_session(AgentSessionStart {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                config_options: serde_json::to_value(&task.config_options)
                    .ok()
                    .filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty)),
                config_option_policy: ConfigOptionPolicy::ReconcileWithAgentDefaults,
                context: Vec::new(),
                cancellation: cancellation.clone(),
                secret_resolver: Some(self.task_secret_resolver(&task.task_id)),
            })
        };
        let session = match &task.agent_session_id {
            Some(session_id) => self
                .agent_gateway
                .resume_session(AgentSessionResume {
                    agent_id: task.agent_id.clone(),
                    task_id: task.task_id.clone(),
                    session_id: session_id.clone(),
                    cwd: task.workspace_root.clone(),
                    model_id: task.model_id.clone(),
                    cancellation: cancellation.clone(),
                })
                .or_else(|_| start())?,
            None => start()?,
        };
        let session_start = TaskSessionStartGuard::new(&self.agent_gateway, session);
        let _ownership = PreparingSessionOwnership::reserve(
            self.preparing_session_ids.clone(),
            session_start.session_id(),
        )?;
        self.turn_runner
            .attach_session_events(task.task_id.clone(), session_start.session_id())?;

        let session_id = session_start.session().session_id.clone();
        let config_options = session_start.session().config_options.clone();
        let config_catalog = session_start.session().config_catalog.clone();
        let commands_catalog = session_start.session().commands_catalog.clone();
        let model_id = session_start.session().model_id.clone();
        let now = now_string();
        let result = self.mutations.commit_existing_task(
            &task.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().tombstoned
                    || ctx.task().agent_session_id != task.agent_session_id
                    || !matches!(ctx.task().preparation, TaskPreparationRecord::Preparing)
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                let task = ctx.task_mut();
                task.agent_session_id = Some(session_id.clone());
                // A fresh start returns an authoritative catalog. Resume only
                // reattaches identity, so missing metadata must preserve the
                // catalog already persisted for the Draft Task.
                if config_catalog.is_some() {
                    task.config_options = config_options.clone();
                    task.config_options_catalog = config_catalog.clone();
                    task.model_id = model_id.clone();
                }
                if task.agent_commands_catalog.is_none() {
                    task.agent_commands_catalog = commands_catalog.clone();
                }
                task.preparation = TaskPreparationRecord::Ready;
                task.updated_at = now.clone();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(RuntimeError::NotReady(
                "Task changed before Agent preparation completed".to_string(),
            ));
        }
        session_start.commit();
        Ok(())
    }

    fn persist_preparation_failure(
        &self,
        task_id: &str,
        error: &RuntimeError,
    ) -> Result<(), RuntimeError> {
        let message = error.to_string();
        let now = now_string();
        self.mutations.commit_existing_task(
            task_id,
            TaskCommitOptions::metadata(),
            move |ctx| {
                if ctx.task().tombstoned
                    || !matches!(ctx.task().preparation, TaskPreparationRecord::Preparing)
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let task = ctx.task_mut();
                task.agent_session_id = None;
                task.preparation = TaskPreparationRecord::Failed { message };
                task.updated_at = now;
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }

    pub(super) fn recover_abandoned_preparations(&self) -> Result<(), RuntimeError> {
        for task in self.store.list_all_task_records()? {
            if !is_abandoned_preparation(&task) {
                continue;
            }
            let message = "Task Agent preparation was interrupted before it finished".to_string();
            self.mutations.commit_existing_task(
                &task.task_id,
                TaskCommitOptions::metadata(),
                move |ctx| {
                    if !is_abandoned_preparation(ctx.task()) {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    ctx.task_mut().preparation = TaskPreparationRecord::Failed { message };
                    Ok(TaskMutationResult::Changed)
                },
            )?;
        }
        Ok(())
    }
}

struct PreparingSessionOwnership {
    session_ids: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    session_id: String,
}

impl PreparingSessionOwnership {
    fn reserve(
        session_ids: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
        session_id: &str,
    ) -> Result<Self, RuntimeError> {
        session_ids
            .lock()
            .map_err(|_| {
                RuntimeError::Internal("preparing session ownership lock poisoned".to_string())
            })?
            .insert(session_id.to_string());
        Ok(Self {
            session_ids,
            session_id: session_id.to_string(),
        })
    }
}

impl Drop for PreparingSessionOwnership {
    fn drop(&mut self) {
        if let Ok(mut session_ids) = self.session_ids.lock() {
            session_ids.remove(&self.session_id);
        }
    }
}

pub(super) fn reject_if_preparation_not_ready(task: &TaskRecord) -> Result<(), ProtocolError> {
    match &task.preparation {
        TaskPreparationRecord::Ready => Ok(()),
        TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing => Err(ProtocolError {
            code: ProtocolErrorCode::Conflict,
            message: "Task Agent preparation is still running".to_string(),
            recoverable: true,
            target: None,
        }),
        TaskPreparationRecord::Failed { message } => Err(ProtocolError {
            code: ProtocolErrorCode::Internal,
            message: format!("Task Agent preparation failed: {message}"),
            recoverable: true,
            target: None,
        }),
    }
}

fn is_abandoned_preparation(task: &TaskRecord) -> bool {
    !task.tombstoned
        && !task.first_prompt_sent
        && task.status == LegacyTaskStatus::Inactive
        && matches!(
            task.preparation,
            TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing
        )
}

pub(super) fn missing_prepared_task_snapshot() -> ProtocolError {
    internal_error("missing task preparation snapshot")
}
