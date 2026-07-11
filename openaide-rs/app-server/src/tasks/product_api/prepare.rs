use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};

use crate::agent::{AgentSessionStart, TurnCancellation};
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
        let session = self.agent_gateway.start_session(AgentSessionStart {
            agent_id: task.agent_id.clone(),
            task_id: task.task_id.clone(),
            cwd: task.workspace_root.clone(),
            model_id: task.model_id.clone(),
            config_options: None,
            context: Vec::new(),
            cancellation: TurnCancellation::new(),
            secret_resolver: Some(self.task_secret_resolver(&task.task_id)),
        })?;
        let session_start = TaskSessionStartGuard::new(&self.agent_gateway, session);
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
                    || ctx.task().agent_session_id.is_some()
                    || !matches!(ctx.task().preparation, TaskPreparationRecord::Preparing)
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                let task = ctx.task_mut();
                task.agent_session_id = Some(session_id.clone());
                for (config_id, value) in &config_options {
                    task.config_options
                        .entry(config_id.clone())
                        .or_insert_with(|| value.clone());
                }
                if task.config_options_catalog.is_none() {
                    task.config_options_catalog = config_catalog.clone();
                }
                if task.agent_commands_catalog.is_none() {
                    task.agent_commands_catalog = commands_catalog.clone();
                }
                task.model_id = task.model_id.clone().or(model_id.clone());
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
