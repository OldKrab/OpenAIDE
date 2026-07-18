use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::ids::{WorktreeId, WorktreeOperationId, WorktreeRepositoryId};
use openaide_app_server_protocol::worktree::{
    WorktreeOperationKind, WorktreeOperationSnapshot, WorktreeOperationState,
};
use uuid::Uuid;

use crate::protocol::errors::RuntimeError;

use super::{
    include_copy, CreateWorktree, RecreateWorktree, StartedWorktreeOperation, WorktreeManager,
};

impl WorktreeManager {
    pub fn start_create(
        &self,
        request: CreateWorktree,
    ) -> Result<StartedWorktreeOperation, RuntimeError> {
        self.start_operation(
            request.repository_id.clone(),
            WorktreeOperationKind::Create,
            move |manager, operation_id| match manager.create_for_operation(request, &operation_id)
            {
                Ok(created) => manager.finish_operation(
                    &created.repository.repository_id,
                    &operation_id,
                    WorktreeOperationState::Succeeded,
                    Some(created.worktree_id),
                    None,
                ),
                Err(error) => manager.finish_operation_by_error(operation_id, error),
            },
        )
    }

    pub fn start_recreate(
        &self,
        request: RecreateWorktree,
    ) -> Result<StartedWorktreeOperation, RuntimeError> {
        let worktree_id = request.worktree_id.clone();
        self.start_operation(
            request.repository_id.clone(),
            WorktreeOperationKind::Recreate,
            move |manager, operation_id| match manager
                .recreate_for_operation(request, &operation_id)
            {
                Ok(repository) => manager.finish_operation(
                    &repository.repository_id,
                    &operation_id,
                    WorktreeOperationState::Succeeded,
                    Some(worktree_id),
                    None,
                ),
                Err(error) => manager.finish_operation_by_error(operation_id, error),
            },
        )
    }

    pub fn start_remove(
        &self,
        repository_id: WorktreeRepositoryId,
        worktree_id: WorktreeId,
    ) -> Result<StartedWorktreeOperation, RuntimeError> {
        self.start_operation(
            repository_id.clone(),
            WorktreeOperationKind::Remove,
            move |manager, operation_id| match manager.remove(&repository_id, &worktree_id) {
                Ok(repository) => manager.finish_operation(
                    &repository.repository_id,
                    &operation_id,
                    WorktreeOperationState::Succeeded,
                    Some(worktree_id),
                    None,
                ),
                Err(error) => manager.finish_operation_by_error(operation_id, error),
            },
        )
    }

    fn start_operation(
        &self,
        repository_id: WorktreeRepositoryId,
        kind: WorktreeOperationKind,
        run: impl FnOnce(WorktreeManager, WorktreeOperationId) + Send + 'static,
    ) -> Result<StartedWorktreeOperation, RuntimeError> {
        // Reject unknown repositories before accepting background work.
        self.snapshot(&repository_id)?;
        let running_stage = match kind {
            WorktreeOperationKind::Create => "Preparing Git worktree",
            WorktreeOperationKind::Recreate => "Recreating Git worktree",
            WorktreeOperationKind::Remove => "Removing worktree",
            WorktreeOperationKind::Refresh => "Refreshing worktrees",
        };
        let operation_id =
            WorktreeOperationId::from(format!("worktree-operation-{}", Uuid::new_v4()));
        let operation = WorktreeOperationSnapshot {
            operation_id: operation_id.clone(),
            kind,
            state: WorktreeOperationState::Queued,
            worktree_id: None,
            stage: Some("Waiting to start".to_string()),
            completed_files: None,
            total_files: None,
            completed_bytes: None,
            total_bytes: None,
            error: None,
        };
        {
            let mut operations = self
                .operations
                .lock()
                .expect("worktree operation lock poisoned");
            let repository_operations = operations.entry(repository_id.clone()).or_default();
            repository_operations.push(operation);
            if repository_operations.len() > 20 {
                repository_operations.remove(0);
            }
        }
        let repository = self.snapshot(&repository_id)?;
        self.updates.repository_updated(repository.clone());
        let manager = self.clone();
        let worker_operation_id = operation_id.clone();
        std::thread::spawn(move || {
            let operation_lock = manager.repository_operation_lock(&repository_id);
            let _operation = operation_lock
                .lock()
                .expect("repository worktree operation lock poisoned");
            manager.update_operation(
                &repository_id,
                &worker_operation_id,
                WorktreeOperationState::Running,
                Some(running_stage.to_string()),
                None,
                None,
            );
            run(manager, worker_operation_id);
        });
        Ok(StartedWorktreeOperation {
            operation_id,
            repository,
        })
    }

    fn repository_operation_lock(&self, repository_id: &WorktreeRepositoryId) -> Arc<Mutex<()>> {
        self.operation_locks
            .lock()
            .expect("worktree operation lock registry poisoned")
            .entry(repository_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn finish_operation_by_error(&self, operation_id: WorktreeOperationId, error: RuntimeError) {
        let repository_id = self
            .operations
            .lock()
            .expect("worktree operation lock poisoned")
            .iter()
            .find_map(|(repository_id, operations)| {
                operations
                    .iter()
                    .any(|operation| operation.operation_id == operation_id)
                    .then(|| repository_id.clone())
            });
        if let Some(repository_id) = repository_id {
            self.finish_operation(
                &repository_id,
                &operation_id,
                WorktreeOperationState::Failed,
                None,
                Some(error.to_string()),
            );
        }
    }

    fn finish_operation(
        &self,
        repository_id: &WorktreeRepositoryId,
        operation_id: &WorktreeOperationId,
        state: WorktreeOperationState,
        worktree_id: Option<WorktreeId>,
        error: Option<String>,
    ) {
        let stage = match state {
            WorktreeOperationState::Succeeded => "Complete",
            WorktreeOperationState::Failed => "Failed",
            WorktreeOperationState::Queued => "Waiting to start",
            WorktreeOperationState::Running => "Preparing Git worktree",
        };
        self.update_operation(
            repository_id,
            operation_id,
            state,
            Some(stage.to_string()),
            worktree_id,
            error,
        );
    }

    fn update_operation(
        &self,
        repository_id: &WorktreeRepositoryId,
        operation_id: &WorktreeOperationId,
        state: WorktreeOperationState,
        stage: Option<String>,
        worktree_id: Option<WorktreeId>,
        error: Option<String>,
    ) {
        if let Some(operation) = self
            .operations
            .lock()
            .expect("worktree operation lock poisoned")
            .get_mut(repository_id)
            .and_then(|operations| {
                operations
                    .iter_mut()
                    .find(|candidate| candidate.operation_id == *operation_id)
            })
        {
            operation.state = state;
            operation.stage = stage;
            operation.worktree_id = worktree_id;
            operation.error = error;
        }
        self.publish_operation(repository_id);
    }

    pub(super) fn update_copy_progress(
        &self,
        repository_id: &WorktreeRepositoryId,
        operation_id: &WorktreeOperationId,
        progress: include_copy::CopyProgress,
    ) {
        if let Some(operation) = self
            .operations
            .lock()
            .expect("worktree operation lock poisoned")
            .get_mut(repository_id)
            .and_then(|operations| {
                operations
                    .iter_mut()
                    .find(|candidate| candidate.operation_id == *operation_id)
            })
        {
            operation.stage = Some("Copying local files".to_string());
            operation.completed_files = Some(progress.completed_files);
            operation.total_files = Some(progress.total_files);
            operation.completed_bytes = Some(progress.completed_bytes);
            operation.total_bytes = Some(progress.total_bytes);
        }
        self.publish_operation(repository_id);
    }

    fn publish_operation(&self, repository_id: &WorktreeRepositoryId) {
        if let Ok(repository) = self.snapshot(repository_id) {
            self.updates.repository_updated(repository);
        }
    }
}
