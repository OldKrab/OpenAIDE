use std::sync::{Arc, Mutex, MutexGuard};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, NormalizedMessage, PermissionDecision, TaskSnapshot};
use crate::storage::records::TaskRecord;
use crate::storage::Store;
use crate::task_events::CommittedTaskDelta;
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::lifecycle::{append_normalized_to_store, upsert_normalized_to_store};
use crate::tasks::runtime_state::RuntimeState;

mod commit;
mod create_validation;

use create_validation::TaskCreationValidationContext;

#[derive(Clone)]
pub(crate) struct TaskMutations {
    store: Store,
    store_update_lock: Arc<Mutex<()>>,
    runtime_state: Arc<Mutex<RuntimeState>>,
    notifier: TaskUpdateNotifier,
}

#[derive(Debug, Clone)]
pub(crate) enum TaskCommitOutcome {
    Committed(TaskCommitFacts),
    Rejected(TaskCommitRejection),
}

#[derive(Debug, Clone)]
pub(crate) struct TaskCommitResult {
    pub outcome: TaskCommitOutcome,
    pub response_snapshot: Option<TaskSnapshot>,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskCommitFacts {
    pub task_id: String,
    pub revision: u64,
    pub committed_task: TaskRecord,
    pub delta: Option<CommittedTaskDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TaskCommitRejection {
    NoChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TaskCommitOptions {
    pub refresh_message_history: bool,
    pub response_snapshot_tail_limit: Option<usize>,
}

impl TaskCommitOptions {
    pub(crate) fn metadata() -> Self {
        Self {
            refresh_message_history: false,
            response_snapshot_tail_limit: None,
        }
    }
}

pub(crate) enum TaskMutationResult {
    Changed,
    Unchanged,
    #[allow(dead_code)]
    Rejected,
}

pub(crate) struct TaskMutationContext<'a> {
    store: &'a Store,
    task: &'a mut TaskRecord,
    committed_delta: Option<CommittedTaskDelta>,
}

impl TaskMutationContext<'_> {
    pub(crate) fn task(&self) -> &TaskRecord {
        self.task
    }

    pub(crate) fn task_mut(&mut self) -> &mut TaskRecord {
        self.task
    }

    pub(crate) fn append_message(&self, message: NormalizedMessage) -> Result<(), RuntimeError> {
        append_normalized_to_store(self.store, &self.task.task_id, message).map(|_| ())
    }

    pub(crate) fn upsert_message(&self, message: NormalizedMessage) -> Result<(), RuntimeError> {
        upsert_normalized_to_store(self.store, &self.task.task_id, message).map(|_| ())
    }

    pub(crate) fn upsert_message_with_record(
        &self,
        message: NormalizedMessage,
    ) -> Result<crate::storage::records::StoredMessage, RuntimeError> {
        upsert_normalized_to_store(self.store, &self.task.task_id, message)
    }

    pub(crate) fn set_committed_delta(&mut self, delta: CommittedTaskDelta) {
        self.committed_delta = Some(delta);
    }

    pub(crate) fn replace_messages(
        &self,
        messages: Vec<NormalizedMessage>,
    ) -> Result<(), RuntimeError> {
        self.store
            .replace_messages_with_normalized(&self.task.task_id, messages)
    }

    pub(crate) fn finish_running_activities(
        &self,
        status: ActivityStatus,
    ) -> Result<bool, RuntimeError> {
        self.store
            .finish_running_activities(&self.task.task_id, status)
    }

    pub(crate) fn finish_running_activity(
        &self,
        identity: &str,
        status: ActivityStatus,
    ) -> Result<bool, RuntimeError> {
        self.store
            .finish_running_activity_by_identity(&self.task.task_id, identity, status)
    }

    pub(crate) fn cancel_pending_permissions(&self) -> Result<bool, RuntimeError> {
        self.store.cancel_pending_permissions(&self.task.task_id)
    }

    pub(crate) fn resolve_permission(
        &self,
        request_id: &str,
        option_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), RuntimeError> {
        self.store
            .resolve_permission(&self.task.task_id, request_id, option_id, decision)
    }

    pub(crate) fn resolve_question(
        &self,
        request_id: &str,
        response: &openaide_app_server_protocol::server_requests::QuestionRequestResponse,
    ) -> Result<bool, RuntimeError> {
        self.store.resolve_question(&self.task.task_id, request_id, response)
    }

    pub(crate) fn cancel_pending_questions(&self) -> Result<bool, RuntimeError> {
        self.store.cancel_pending_questions(&self.task.task_id)
    }
}

impl TaskMutations {
    pub(crate) fn new(
        store: Store,
        store_update_lock: Arc<Mutex<()>>,
        runtime_state: Arc<Mutex<RuntimeState>>,
        notifier: TaskUpdateNotifier,
    ) -> Self {
        Self {
            store,
            store_update_lock,
            runtime_state,
            notifier,
        }
    }

    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, ()> {
        self.store_update_lock
            .lock()
            .expect("store update lock poisoned")
    }

    #[cfg(test)]
    pub(crate) fn current_revision(&self) -> u64 {
        self.runtime_state
            .lock()
            .expect("runtime state poisoned")
            .current_revision()
    }

    pub(crate) fn commit_existing_task(
        &self,
        task_id: &str,
        options: TaskCommitOptions,
        mutation: impl FnOnce(&mut TaskMutationContext<'_>) -> Result<TaskMutationResult, RuntimeError>,
    ) -> Result<TaskCommitResult, RuntimeError> {
        commit::commit_existing_task(self, task_id, options, mutation)
    }

    pub(crate) fn create_task(
        &self,
        task: TaskRecord,
        initial_messages: Vec<NormalizedMessage>,
        options: TaskCommitOptions,
    ) -> Result<TaskCommitResult, RuntimeError> {
        self.create_task_with_validation(task, initial_messages, options, |_| Ok(()))
    }

    #[allow(dead_code)]
    pub(crate) fn publish_current_task(&self, task_id: &str) -> Result<(), RuntimeError> {
        commit::publish_current_task(self, task_id)
    }

    pub(crate) fn create_task_with_validation(
        &self,
        task: TaskRecord,
        initial_messages: Vec<NormalizedMessage>,
        options: TaskCommitOptions,
        validate: impl FnOnce(&TaskCreationValidationContext<'_>) -> Result<(), RuntimeError>,
    ) -> Result<TaskCommitResult, RuntimeError> {
        self.create_task_with_validation_and_writer(
            task,
            initial_messages,
            options,
            validate,
            |store, task| store.write_task(task),
        )
    }

    fn create_task_with_validation_and_writer(
        &self,
        task: TaskRecord,
        initial_messages: Vec<NormalizedMessage>,
        options: TaskCommitOptions,
        validate: impl FnOnce(&TaskCreationValidationContext<'_>) -> Result<(), RuntimeError>,
        write_task: impl FnOnce(&Store, &TaskRecord) -> Result<(), RuntimeError>,
    ) -> Result<TaskCommitResult, RuntimeError> {
        commit::create_task_with_validation_and_writer(
            self,
            task,
            initial_messages,
            options,
            validate,
            write_task,
        )
    }

    #[cfg(test)]
    pub(crate) fn append_message(
        &self,
        task_id: &str,
        message: NormalizedMessage,
    ) -> Result<(), RuntimeError> {
        append_normalized_to_store(&self.store, task_id, message).map(|_| ())
    }
}

#[cfg(test)]
mod tests;
