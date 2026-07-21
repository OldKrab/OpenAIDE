use std::sync::{Arc, Mutex, MutexGuard};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, AgentMessagePart, ChatMessage, NormalizedMessage, TaskSnapshot,
    ToolPermissionOutcome,
};
use crate::storage::cursor;
use crate::storage::records::{StoredMessage, TaskRecord};
use crate::storage::task_journal::TaskProjection;
use crate::storage::tool_artifacts::{extract_tool_artifacts, PersistedToolDetail};
use crate::storage::Store;
use crate::task_events::TaskUpdateNotifier;
use crate::task_events::{CommittedChatChange, CommittedTaskChange, ToolDetailUpdate};
#[cfg(test)]
use crate::tasks::lifecycle::append_normalized_to_store;
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

pub(crate) enum AgentMessageAppend {
    Appended(StoredMessage),
    TextAppended { message_id: String },
    PartAppended(StoredMessage),
}

pub(crate) struct TaskMutationContext<'a> {
    projection: &'a mut TaskProjection,
    artifact_replacements: Vec<PersistedToolDetail>,
    terminal_appends: Vec<crate::storage::task_journal::ToolTerminalAppend>,
    chat_changes: Vec<CommittedChatChange>,
    tool_details: Vec<ToolDetailUpdate>,
}

impl TaskMutationContext<'_> {
    pub(crate) fn task(&self) -> &TaskRecord {
        &self.projection.task
    }

    pub(crate) fn task_mut(&mut self) -> &mut TaskRecord {
        &mut self.projection.task
    }

    pub(crate) fn append_message(
        &mut self,
        message: NormalizedMessage,
    ) -> Result<(), RuntimeError> {
        let mut message = message;
        self.artifact_replacements
            .extend(extract_tool_artifacts(&mut message));
        let sequence = self
            .projection
            .messages
            .last()
            .map(|message| message.sequence + 1)
            .unwrap_or(1);
        let identity = message.identity();
        let stored = StoredMessage {
            sequence,
            chat: ChatMessage {
                cursor: cursor::from_sequence(sequence),
                message_id: identity.clone(),
                identity,
                message_type: message.message_type().to_string(),
                message,
            },
        };
        self.projection.messages.push(stored.clone());
        crate::storage::message_store::advance_message_meta(self.projection, 0);
        self.chat_changes.push(CommittedChatChange::Append {
            item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
        });
        Ok(())
    }

    /// Appends a pre-identified Chat row while the surrounding workflow owns
    /// its message-id/identity distinction.
    pub(crate) fn append_chat_message(&mut self, mut message: ChatMessage) {
        let sequence = self
            .projection
            .messages
            .last()
            .map(|stored| stored.sequence + 1)
            .unwrap_or(1);
        message.cursor = cursor::from_sequence(sequence);
        self.projection.messages.push(StoredMessage {
            sequence,
            chat: message,
        });
        crate::storage::message_store::advance_message_meta(self.projection, 0);
    }

    pub(crate) fn append_terminal(
        &mut self,
        artifact_id: String,
        terminal_id: String,
        data: String,
    ) {
        self.terminal_appends
            .push(crate::storage::task_journal::ToolTerminalAppend {
                artifact_id,
                terminal_id,
                data,
            });
    }

    pub(crate) fn record_tool_permission_outcome(
        &mut self,
        activity_identity: &str,
        tool_call_id: &str,
        outcome: ToolPermissionOutcome,
    ) -> Result<bool, RuntimeError> {
        let changed = record_permission_outcome(
            &mut self.projection.messages,
            activity_identity,
            tool_call_id,
            outcome,
        );
        if !changed.is_empty() {
            crate::storage::message_store::advance_message_meta(self.projection, 0);
        }
        self.chat_changes
            .extend(changed.iter().map(|stored| CommittedChatChange::Upsert {
                item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
            }));
        Ok(!changed.is_empty())
    }

    pub(crate) fn upsert_message_with_details(
        &mut self,
        message: NormalizedMessage,
    ) -> Result<crate::tasks::lifecycle::UpsertedMessage, RuntimeError> {
        let mut message = message;
        let tool_details = extract_tool_artifacts(&mut message);
        self.artifact_replacements.extend(tool_details.clone());
        let identity = message.identity();
        let mut chat = ChatMessage {
            cursor: String::new(),
            message_id: identity.clone(),
            identity,
            message_type: message.message_type().to_string(),
            message,
        };
        let stored = if let Some(stored) = self
            .projection
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == chat.identity)
        {
            chat.message
                .preserve_tool_permission_outcomes_from(&stored.chat.message);
            chat.cursor = stored.chat.cursor.clone();
            chat.message_id = stored.chat.message_id.clone();
            chat.message.preserve_created_at_from(&stored.chat.message);
            stored.chat = chat;
            stored.clone()
        } else {
            let sequence = self
                .projection
                .messages
                .last()
                .map(|message| message.sequence + 1)
                .unwrap_or(1);
            chat.cursor = cursor::from_sequence(sequence);
            let stored = StoredMessage { sequence, chat };
            self.projection.messages.push(stored.clone());
            stored
        };
        crate::storage::message_store::advance_message_meta(self.projection, 0);
        let upserted = crate::tasks::lifecycle::UpsertedMessage {
            stored,
            tool_details,
        };
        self.chat_changes.push(CommittedChatChange::Upsert {
            item: crate::snapshots::task_snapshot::project_chat_item(&upserted.stored.chat),
        });
        self.tool_details
            .extend(upserted.tool_details.iter().map(|detail| ToolDetailUpdate {
                artifact_id: detail.artifact_id.clone(),
                details: crate::snapshots::task_snapshot::project_tool_details(&detail.details),
                terminal_appends: Vec::new(),
            }));
        Ok(upserted)
    }

    pub(crate) fn append_agent_message_part(
        &mut self,
        message: NormalizedMessage,
    ) -> Result<AgentMessageAppend, RuntimeError> {
        let text = match &message {
            NormalizedMessage::AgentMessage { parts, .. } => match parts.as_slice() {
                [crate::protocol::model::AgentMessagePart::Text { text }] => Some(text.clone()),
                _ => None,
            },
            _ => None,
        };
        let identity = message.identity();
        let result = if let Some(stored) = self
            .projection
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == identity)
        {
            let text_appended = append_agent_part(&mut stored.chat.message, message)?;
            let updated = stored.clone();
            if text_appended {
                AgentMessageAppend::TextAppended {
                    message_id: updated.chat.message_id,
                }
            } else {
                AgentMessageAppend::PartAppended(updated)
            }
        } else {
            let sequence = self
                .projection
                .messages
                .last()
                .map(|message| message.sequence + 1)
                .unwrap_or(1);
            let stored = StoredMessage {
                sequence,
                chat: ChatMessage {
                    cursor: cursor::from_sequence(sequence),
                    message_id: identity.clone(),
                    identity,
                    message_type: message.message_type().to_string(),
                    message,
                },
            };
            self.projection.messages.push(stored.clone());
            AgentMessageAppend::Appended(stored)
        };
        crate::storage::message_store::advance_message_meta(self.projection, 0);
        match &result {
            AgentMessageAppend::Appended(stored) => {
                self.chat_changes.push(CommittedChatChange::Append {
                    item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
                });
            }
            AgentMessageAppend::TextAppended { message_id } => {
                self.chat_changes.push(CommittedChatChange::AppendText {
                    message_id: message_id.clone().into(),
                    text: text.expect("text append result requires one incoming text part"),
                });
            }
            AgentMessageAppend::PartAppended(stored) => {
                self.chat_changes.push(CommittedChatChange::Upsert {
                    item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
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
        let existing_ids = self
            .projection
            .messages
            .iter()
            .map(|message| {
                (
                    message.chat.identity.clone(),
                    message.chat.message_id.clone(),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let mut stored_messages = Vec::with_capacity(messages.len());
        for (index, mut message) in messages.into_iter().enumerate() {
            self.artifact_replacements
                .extend(extract_tool_artifacts(&mut message));
            let sequence = index as u64 + 1;
            let identity = message.identity();
            stored_messages.push(StoredMessage {
                sequence,
                chat: ChatMessage {
                    cursor: cursor::from_sequence(sequence),
                    message_id: existing_ids
                        .get(&identity)
                        .cloned()
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    identity,
                    message_type: message.message_type().to_string(),
                    message,
                },
            });
        }
        self.projection.messages = stored_messages;
        crate::storage::message_store::advance_message_meta(self.projection, native_updated_at);
        self.chat_changes.push(CommittedChatChange::Replace);
        Ok(())
    }

    pub(crate) fn finish_running_activities(
        &mut self,
        status: ActivityStatus,
    ) -> Result<bool, RuntimeError> {
        let mut changed = Vec::new();
        for stored in self.projection.messages.iter_mut().rev() {
            if finish_running_activity(&mut stored.chat.message, status) {
                changed.push(stored.clone());
            }
        }
        if !changed.is_empty() {
            crate::storage::message_store::advance_message_meta(self.projection, 0);
        }
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
        let changed = match self
            .projection
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == identity)
        {
            Some(stored) => finish_running_activity(&mut stored.chat.message, status)
                .then(|| stored.clone())
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        if !changed.is_empty() {
            crate::storage::message_store::advance_message_meta(self.projection, 0);
        }
        self.chat_changes
            .extend(changed.iter().map(|stored| CommittedChatChange::Upsert {
                item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
            }));
        Ok(!changed.is_empty())
    }
}

fn append_agent_part(
    existing: &mut NormalizedMessage,
    incoming: NormalizedMessage,
) -> Result<bool, RuntimeError> {
    match (existing, incoming) {
        (
            NormalizedMessage::AgentMessage { role, parts, .. },
            NormalizedMessage::AgentMessage {
                role: incoming_role,
                parts: incoming_parts,
                ..
            },
        ) if *role == incoming_role && incoming_parts.len() == 1 => {
            let part = incoming_parts.into_iter().next().expect("one checked part");
            if let (Some(AgentMessagePart::Text { text }), AgentMessagePart::Text { text: chunk }) =
                (parts.last_mut(), &part)
            {
                text.push_str(chunk);
                Ok(true)
            } else {
                parts.push(part);
                Ok(false)
            }
        }
        _ => Err(RuntimeError::Conflict(
            "ACP message id changed content channel".to_string(),
        )),
    }
}

fn record_permission_outcome(
    messages: &mut [StoredMessage],
    activity_identity: &str,
    tool_call_id: &str,
    outcome: ToolPermissionOutcome,
) -> Vec<StoredMessage> {
    let Some(stored) = messages
        .iter_mut()
        .find(|stored| stored.chat.identity == activity_identity)
    else {
        return Vec::new();
    };
    let NormalizedMessage::Activity { steps, .. } = &mut stored.chat.message else {
        return Vec::new();
    };
    let Some(outcomes) = steps.iter_mut().find_map(|step| match step {
        ActivityStep::Tool {
            tool_call_id: Some(id),
            permission_outcomes,
            ..
        } if id == tool_call_id => Some(permission_outcomes),
        _ => None,
    }) else {
        return Vec::new();
    };
    if let Some(existing) = outcomes
        .iter_mut()
        .find(|existing| existing.request_id == outcome.request_id)
    {
        *existing = outcome;
    } else {
        outcomes.push(outcome);
    }
    vec![stored.clone()]
}

fn finish_running_activity(message: &mut NormalizedMessage, status: ActivityStatus) -> bool {
    let NormalizedMessage::Activity {
        status: activity_status,
        steps,
        ..
    } = message
    else {
        return false;
    };
    if *activity_status != ActivityStatus::Running {
        return false;
    }
    *activity_status = status;
    for step in steps {
        match step {
            ActivityStep::Tool {
                status: step_status,
                ..
            }
            | ActivityStep::Command {
                status: step_status,
                ..
            } if *step_status == ActivityStatus::Running => *step_status = status,
            _ => {}
        }
    }
    true
}

impl TaskMutations {
    pub(crate) fn new(
        store: Store,
        store_update_lock: Arc<Mutex<()>>,
        runtime_state: Arc<Mutex<RuntimeState>>,
        notifier: TaskUpdateNotifier,
    ) -> Self {
        let publication_notifier = notifier.clone();
        store.set_task_commit_handler(Arc::new(move |committed| {
            for change in committed.artifact_changes {
                // A structured replacement and its same-revision terminal
                // bytes must remain one atomic synchronous Tool delta. Other
                // stream appends in the physical batch keep their async owner.
                let synchronously_owned = committed.task_snapshot_changed
                    && committed
                        .replaced_artifact_ids
                        .contains(&change.artifact_id);
                if !synchronously_owned && !change.terminal_appends.is_empty() {
                    publication_notifier.tool_detail_changed(
                        &committed.task_id,
                        change.artifact_id,
                        change.artifact_sequence,
                        change.terminal_appends,
                    );
                }
            }
        }));
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

    /// Compacts streamed Chat deltas only at an explicit workflow boundary.
    pub(crate) fn compact_message_journal(&self, task_id: &str) -> Result<(), RuntimeError> {
        let _guard = self.lock();
        self.store.compact_message_journal(task_id)?;
        Ok(())
    }

    /// Durably appends terminal output without advancing Task revision.
    pub(crate) fn append_terminal_outputs(
        &self,
        task_id: &str,
        expected_session_id: &str,
        appends: Vec<crate::storage::task_journal::ToolTerminalAppend>,
    ) -> Result<(), RuntimeError> {
        let _guard = self.lock();
        let projection = self.store.task_journal().load(task_id)?;
        if projection.task.agent_session_id.as_deref() != Some(expected_session_id) {
            return Err(RuntimeError::Conflict(
                "Terminal update belongs to a stale Native Session".to_string(),
            ));
        }
        let receipt = self.store.task_journal().submit(
            crate::storage::task_journal::TaskWrite::stream_append_terminals(
                task_id.to_string(),
                appends,
            ),
        )?;
        drop(receipt);
        Ok(())
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

    /// Leases a matching free Prepared Task or creates one under the same storage lock.
    /// Task lifecycle records remain the only authoritative ownership representation.
    pub(crate) fn acquire_prepared_task(
        &self,
        task: TaskRecord,
        initial_messages: Vec<NormalizedMessage>,
        options: TaskCommitOptions,
    ) -> Result<TaskCommitResult, RuntimeError> {
        commit::acquire_prepared_task(self, task, initial_messages, options)
    }

    /// Releases only the named lease and applies free-pool retention atomically.
    pub(crate) fn release_prepared_task(
        &self,
        client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        task_id: &str,
        now: &str,
    ) -> Result<Vec<TaskRecord>, RuntimeError> {
        commit::release_prepared_task(self, client_instance_id, task_id, now)
    }

    /// Repairs the derived free pool from durable Task records.
    pub(crate) fn reconcile_prepared_task_pool(
        &self,
        clear_leases: bool,
    ) -> Result<Vec<TaskRecord>, RuntimeError> {
        commit::reconcile_prepared_task_pool(self, clear_leases)
    }

    pub(crate) fn dispose_prepared_tasks_for_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<TaskRecord>, RuntimeError> {
        commit::dispose_prepared_tasks_for_agent(self, agent_id)
    }

    /// Removes invisible leased/free Tasks before their worktree directory is deleted.
    pub(crate) fn dispose_prepared_tasks_for_worktree(
        &self,
        worktree_id: &str,
    ) -> Result<Vec<TaskRecord>, RuntimeError> {
        commit::dispose_prepared_tasks_for_worktree(self, worktree_id)
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
            |_store, _task| Ok(()),
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
