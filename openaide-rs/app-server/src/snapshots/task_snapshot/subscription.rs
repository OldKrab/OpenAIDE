//! Answers a Task subscription (ADR-0086): every current field, and the Chat as `current`,
//! `deltas` after the client's known version, or `full` (the tail page).
//!
//! Fields carry no version and are always complete. Only the Chat is versioned, by the number of
//! its last `chat.log` frame, which is durable, so a copy cached before an App Server restart
//! resumes the same way as one cached a moment ago.

use std::time::Instant;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::events::TaskChatChange;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::state::TaskChatSync;
use serde_json::json;

use super::{
    project_chat_item, project_stored_task_snapshot_with_history_sync, subagent_overview,
    task_snapshot_error, TaskSnapshotStore,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::MessagePage;
use crate::snapshots::TaskSubscription;
use crate::storage::records::StoredMessage;
use crate::storage::task_journal::{ChatChangesSince, ChatFrameChanges, TaskOperation};
use crate::tasks::snapshot::snapshot_from_record_and_chat;

/// Why a subscription answered with the whole Chat.
#[derive(Debug, Clone, Copy)]
enum FullReason {
    NoKnownVersion,
    TooOld,
    Ahead,
    /// The frames could not be projected or read; the whole Chat is always a correct answer.
    Unresolvable,
}

impl FullReason {
    fn name(self) -> &'static str {
        match self {
            Self::NoKnownVersion => "noKnownVersion",
            Self::TooOld => "tooOld",
            Self::Ahead => "ahead",
            Self::Unresolvable => "unresolvable",
        }
    }
}

/// What one committed projection says, read without cloning its Chat.
struct ProjectionFacts {
    task: crate::storage::records::TaskRecord,
    chat_version: u64,
    total_count: u64,
    history_version: u64,
}

impl TaskSnapshotStore {
    pub(super) fn subscription(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        known_version: Option<u64>,
    ) -> Result<TaskSubscription, ProtocolError> {
        let started = Instant::now();
        let authorize = |task: &crate::storage::records::TaskRecord| {
            crate::tasks::access::require_client_task_access(task, client_instance_id)
        };
        let Some(known) = known_version else {
            return self.full_subscription(
                task_id,
                authorize,
                FullReason::NoKnownVersion,
                None,
                started,
            );
        };
        let facts = self
            .store
            .task_journal()
            .inspect_projection(task_id.as_str(), |projection| ProjectionFacts {
                task: projection.task.clone(),
                chat_version: projection.chat_version,
                total_count: projection.messages.len() as u64,
                history_version: projection.message_meta.version,
            })
            .map_err(task_snapshot_error)?;
        // A removed or another client's Prepared Task answers exactly like its baseline.
        self.ensure_visible(&facts.task, authorize)?;
        let sync = if known > facts.chat_version {
            Err(FullReason::Ahead)
        } else if known == facts.chat_version {
            Ok(TaskChatSync::Current)
        } else {
            self.deltas_since(task_id, known, facts.chat_version)
        };
        match sync {
            Ok(chat_sync) => {
                let chat_version = facts.chat_version;
                let page = MessagePage {
                    task_id: task_id.as_str().to_string(),
                    items: Vec::new(),
                    has_before: false,
                    total_count: facts.total_count,
                    version: facts.history_version,
                    chat_version,
                    start_cursor: None,
                    end_cursor: None,
                };
                let task = self.project_with_overlays(
                    snapshot_from_record_and_chat(facts.task, page),
                    task_id,
                )?;
                log_resolved(
                    task_id,
                    Some(known),
                    chat_version,
                    &chat_sync,
                    None,
                    started,
                );
                Ok(TaskSubscription {
                    task,
                    chat_version,
                    chat_sync,
                })
            }
            Err(reason) => self.full_subscription(task_id, authorize, reason, Some(known), started),
        }
    }

    fn full_subscription(
        &self,
        task_id: &TaskId,
        authorize: impl FnOnce(&crate::storage::records::TaskRecord) -> Result<(), RuntimeError>,
        reason: FullReason,
        known: Option<u64>,
        started: Instant,
    ) -> Result<TaskSubscription, ProtocolError> {
        let (task, chat_version) = self.open_authorized_with_version(task_id, authorize)?;
        log_resolved(
            task_id,
            known,
            chat_version,
            &TaskChatSync::Full,
            Some(reason),
            started,
        );
        Ok(TaskSubscription {
            task,
            chat_version,
            chat_sync: TaskChatSync::Full,
        })
    }

    /// The projected operations of the frames after `known` up to `through`, or the reason the
    /// client must take the whole Chat.
    fn deltas_since(
        &self,
        task_id: &TaskId,
        known: u64,
        through: u64,
    ) -> Result<TaskChatSync, FullReason> {
        let journal = self.store.task_journal();
        let frames = match journal.chat_changes_since(task_id.as_str(), known, through) {
            Ok(ChatChangesSince::Deltas { frames, .. }) => frames,
            Ok(ChatChangesSince::Current { .. }) => return Ok(TaskChatSync::Current),
            Ok(ChatChangesSince::TooOld { .. }) => return Err(FullReason::TooOld),
            Ok(ChatChangesSince::Ahead { .. }) => return Err(FullReason::Ahead),
            // The store logged the failure.
            Err(_) => return Err(FullReason::Unresolvable),
        };
        // Messages only grow between replacements, so a later projection still holds every
        // message these frames touched; one that does not makes the frames unusable.
        let changes = journal
            .inspect_projection(task_id.as_str(), |projection| {
                project_frames(&frames, &projection.messages)
            })
            .ok()
            .flatten();
        changes
            .map(|changes| TaskChatSync::Deltas { changes })
            .ok_or(FullReason::Unresolvable)
    }

    /// Adds the process-local state that does not live in the Task record.
    fn project_with_overlays(
        &self,
        stored: crate::protocol::model::TaskSnapshot,
        task_id: &TaskId,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let history_sync = self.history_sync.history_sync_snapshot(task_id.as_str());
        let mut projected = project_stored_task_snapshot_with_history_sync(stored, history_sync)?;
        projected.subagents = subagent_overview(&self.store, task_id.as_str());
        Ok(projected)
    }

    pub(super) fn open_authorized_with_version(
        &self,
        task_id: &TaskId,
        authorize: impl FnOnce(&crate::storage::records::TaskRecord) -> Result<(), RuntimeError>,
    ) -> Result<(openaide_app_server_protocol::snapshot::TaskSnapshot, u64), ProtocolError> {
        let (record, page) = self
            .store
            .read_task_with_tail(task_id.as_str(), self.tail_limit)
            .map_err(task_snapshot_error)?;
        self.ensure_visible(&record, authorize)?;
        let chat_version = page.chat_version;
        let task =
            self.project_with_overlays(snapshot_from_record_and_chat(record, page), task_id)?;
        Ok((task, chat_version))
    }

    fn ensure_visible(
        &self,
        task: &crate::storage::records::TaskRecord,
        authorize: impl FnOnce(&crate::storage::records::TaskRecord) -> Result<(), RuntimeError>,
    ) -> Result<(), ProtocolError> {
        if task.tombstoned {
            return Err(task_snapshot_error(RuntimeError::TaskNotFound(
                task.task_id.clone(),
            )));
        }
        authorize(task).map_err(task_snapshot_error)
    }
}

/// Projects frame operations into the changes a live subscriber receives for the same
/// transactions. `None` when an operation is not a delta or a touched message is missing.
fn project_frames(
    frames: &[ChatFrameChanges],
    messages: &[StoredMessage],
) -> Option<Vec<TaskChatChange>> {
    let mut changes = Vec::new();
    for frame in frames {
        for operation in &frame.operations {
            match operation {
                TaskOperation::AppendMessage { message } => changes.push(TaskChatChange::Append {
                    item: project_chat_item(&message.chat),
                }),
                TaskOperation::UpsertMessage { message } => changes.push(TaskChatChange::Upsert {
                    item: project_chat_item(&message.chat),
                }),
                TaskOperation::AppendText { identity, text, .. } => {
                    let message_id = messages
                        .iter()
                        .find(|stored| stored.chat.identity == *identity)?
                        .chat
                        .message_id
                        .clone();
                    changes.push(TaskChatChange::AppendText {
                        message_id: message_id.into(),
                        text: text.clone(),
                    });
                }
                // Neither changes a Chat item.
                TaskOperation::ReplaceMessageMeta { .. } | TaskOperation::CommitArtifact { .. } => {
                }
                // A replacement is read as `TooOld`; the rest never reach a Chat frame.
                TaskOperation::ReplaceMessages { .. }
                | TaskOperation::ReplaceProjection { .. }
                | TaskOperation::ReplaceTask { .. }
                | TaskOperation::Create { .. } => return None,
            }
        }
    }
    Some(changes)
}

/// One event per Task subscription explains whether a switch paid for the whole Chat.
fn log_resolved(
    task_id: &TaskId,
    known: Option<u64>,
    chat_version: u64,
    sync: &TaskChatSync,
    full_reason: Option<FullReason>,
    started: Instant,
) {
    let (outcome, change_count) = match sync {
        TaskChatSync::Current => ("current", 0),
        TaskChatSync::Deltas { changes } => ("deltas", changes.len()),
        TaskChatSync::Full => ("full", 0),
    };
    crate::logging::info(
        "task_subscription_resolved",
        json!({
            "operation": "task_subscribe",
            "task_id": task_id.as_str(),
            "outcome": outcome,
            "full_reason": full_reason.map(FullReason::name),
            "known_version": known,
            "chat_version": chat_version,
            "change_count": change_count,
            "duration_ms": started.elapsed().as_millis(),
        }),
    );
}
