use std::sync::{Arc, Mutex, MutexGuard};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, NormalizedMessage, TaskSnapshot};
use crate::storage::records::TaskRecord;
use crate::storage::Store;
use crate::task_events::TaskUpdateNotifier;
use crate::task_events::{CommittedChatChange, CommittedTaskChange, ToolDetailUpdate};
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
// Commit facts are passed directly from the serialized mutation boundary;
// rejected commits stay allocation-free and do not justify boxing all success.
#[allow(clippy::large_enum_variant)]
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
    pub change: CommittedTaskChange,
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
    chat_changes: Vec<CommittedChatChange>,
    tool_details: Vec<ToolDetailUpdate>,
}

impl TaskMutationContext<'_> {
    pub(crate) fn task(&self) -> &TaskRecord {
        self.task
    }

    pub(crate) fn task_mut(&mut self) -> &mut TaskRecord {
        self.task
    }

    pub(crate) fn append_message(
        &mut self,
        message: NormalizedMessage,
    ) -> Result<(), RuntimeError> {
        let stored = append_normalized_to_store(self.store, &self.task.task_id, message)?;
        self.chat_changes.push(CommittedChatChange::Append {
            item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
        });
        Ok(())
    }

    pub(crate) fn upsert_message_with_details(
        &mut self,
        message: NormalizedMessage,
    ) -> Result<crate::tasks::lifecycle::UpsertedMessage, RuntimeError> {
        let upserted = upsert_normalized_to_store(self.store, &self.task.task_id, message)?;
        self.chat_changes.push(CommittedChatChange::Upsert {
            item: crate::snapshots::task_snapshot::project_chat_item(&upserted.stored.chat),
        });
        self.tool_details
            .extend(upserted.tool_details.iter().map(|detail| ToolDetailUpdate {
                artifact_id: detail.artifact_id.clone(),
                details: crate::snapshots::task_snapshot::project_tool_details(&detail.details),
            }));
        Ok(upserted)
    }

    pub(crate) fn append_text_chunk(
        &mut self,
        message: NormalizedMessage,
    ) -> Result<crate::storage::message_store::TextChunkAppend, RuntimeError> {
        let text = match &message {
            NormalizedMessage::AgentText { text, .. } | NormalizedMessage::Thought { text, .. } => {
                text.clone()
            }
            _ => String::new(),
        };
        let result = self.store.append_text_chunk(&self.task.task_id, message)?;
        match &result {
            crate::storage::message_store::TextChunkAppend::Appended(stored) => {
                self.chat_changes.push(CommittedChatChange::Append {
                    item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
                });
            }
            crate::storage::message_store::TextChunkAppend::Updated(stored) => {
                self.chat_changes.push(CommittedChatChange::AppendText {
                    message_id: stored.chat.message_id.clone().into(),
                    text,
                });
            }
        }
        Ok(result)
    }

    pub(crate) fn replace_messages_from_native_session(
        &mut self,
        messages: Vec<NormalizedMessage>,
        native_updated_at: u128,
    ) -> Result<(), RuntimeError> {
        self.store.replace_messages_with_normalized_at(
            &self.task.task_id,
            messages,
            native_updated_at,
        )?;
        self.chat_changes.push(CommittedChatChange::Replace);
        Ok(())
    }

    pub(crate) fn finish_running_activities(
        &mut self,
        status: ActivityStatus,
    ) -> Result<bool, RuntimeError> {
        let changed = self
            .store
            .finish_running_activities(&self.task.task_id, status)?;
        self.chat_changes
            .extend(changed.iter().map(|stored| CommittedChatChange::Upsert {
                item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
            }));
        Ok(!changed.is_empty())
    }

    /// Finishes only the App Server-owned working marker for this prompt.
    /// Agent tool activity remains session-owned and may receive later updates.
    pub(crate) fn finish_running_activity(
        &mut self,
        identity: &str,
        status: ActivityStatus,
    ) -> Result<bool, RuntimeError> {
        let changed =
            self.store
                .finish_running_activity_by_identity(&self.task.task_id, identity, status)?;
        self.chat_changes
            .extend(changed.iter().map(|stored| CommittedChatChange::Upsert {
                item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
            }));
        Ok(!changed.is_empty())
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

    /// Resolves one client-owned New Task or creates it while holding the same storage lock.
    /// This keeps lookup and creation atomic without adding a second ownership index.
    pub(crate) fn resolve_or_create_new_task(
        &self,
        task: TaskRecord,
        initial_messages: Vec<NormalizedMessage>,
        options: TaskCommitOptions,
    ) -> Result<TaskCommitResult, RuntimeError> {
        commit::resolve_or_create_new_task(self, task, initial_messages, options)
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
