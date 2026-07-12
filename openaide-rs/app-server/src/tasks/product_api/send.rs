use openaide_app_server_protocol::errors::{ErrorTarget, ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, MessageId, TurnId};
use openaide_app_server_protocol::task::TaskSendParams;
use uuid::Uuid;

use crate::attachment_runtime::{AttachmentOwner, ResolvedSendAttachments};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::storage::records::{TaskLifecycle, TaskPreparationRecord, TaskRecord};
use crate::storage::send_receipts::TaskSendReceipt;
use crate::task_recovery::RESTART_INTERRUPTION_MESSAGE;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::tasks::snapshot::build_snapshot;
use crate::tasks::transitions::TaskTransitions;
use crate::time::now_string;

use super::{
    conflict_error, internal_error, runtime_error, storage_error, validation_error, TaskProductApi,
    TaskSendAccepted,
};

mod session;

use revision_guard::same_send_target;

pub(crate) mod committed;
mod recovery;
mod revision_guard;
mod support;

use committed::CommittedSend;
use support::{protocol_error_from_attachment_runtime, send_identity, SendFingerprint};

#[derive(Clone)]
pub(crate) struct PendingSendSync {
    pub(crate) prompt_text: String,
    pub(crate) attachments: ResolvedSendAttachments,
    pub(crate) committed_send: CommittedSend,
}

impl TaskProductApi {
    pub(super) fn send_message(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSendParams,
    ) -> Result<TaskSendAccepted, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.turn_acceptance.serialize(&task_id, || {
            self.send_message_serialized(client_instance_id, params)
        })
    }

    fn send_message_serialized(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSendParams,
    ) -> Result<TaskSendAccepted, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let mut existing_task = self.read_task_for_client(&task_id, client_instance_id)?;
        let idempotency_key = params.idempotency_key.as_str().to_string();
        let idempotency_identity = send_identity(&idempotency_key);
        if let Some(existing) = self.existing_send(&task_id, &idempotency_key)? {
            existing.validate(&SendFingerprint::from_request_message(&params.message))?;
            self.recover_existing_send(&task_id, &existing)?;
            let snapshot = build_snapshot(&self.store, &task_id, 100).map_err(storage_error)?;
            let task = self.project_task_snapshot(snapshot)?;
            return Ok(TaskSendAccepted {
                task,
                turn_id: existing.turn_id,
                user_message_id: existing.user_message_id,
            });
        }
        let recovered_stale_turn =
            if let Some(active_turn_id) = existing_task.active_turn_id.clone() {
                if self
                    .turn_runner
                    .active_turn_is_live(&task_id, active_turn_id.as_str())
                    || self
                        .turn_acceptance
                        .owns_pending_turn(&task_id, active_turn_id.as_str())
                {
                    false
                } else {
                    self.recover_stale_active_turn(&task_id, active_turn_id.as_str())?;
                    existing_task = self.store.read_task(&task_id).map_err(runtime_error)?;
                    true
                }
            } else {
                false
            };
        if existing_task.active_turn_id.is_some() {
            return Err(conflict_error("Task is already running"));
        }
        if !recovered_stale_turn && existing_task.revision != params.task_revision {
            return Err(self.stale_task_revision_error(&task_id)?);
        }
        let attachment_owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        let attachment_reservation = self
            .attachments
            .reserve_for_send(&attachment_owner, &params.message.attachments)
            .map_err(protocol_error_from_attachment_runtime)?;
        let fingerprint =
            SendFingerprint::from_message(&params.message, attachment_reservation.attachments())?;
        self.agent_registry
            .require(&existing_task.agent_id)
            .map_err(super::protocol_error_from_runtime)?;
        let now = now_string();
        let turn_id = TurnId::from(format!("turn_{}", Uuid::new_v4()));
        let user_message_id = MessageId::from(format!("message_{}", Uuid::new_v4()));
        if !self
            .turn_acceptance
            .own_pending_turn(&task_id, turn_id.as_str())
        {
            return Err(conflict_error("Task is already starting a Turn"));
        }
        let mut sync_generation = None;
        let sending_client = client_instance_id.clone();
        let commit_result =
            self.mutations
                .commit_existing_task(&task_id, durable_send_commit_options(), |ctx| {
                    if ctx.task().tombstoned {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if crate::tasks::access::require_client_task_access(ctx.task(), &sending_client)
                        .is_err()
                    {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if ctx.task().active_turn_id.is_some() {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if !same_send_target(&existing_task, ctx.task()) {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    // Acquire synchronization ownership only after the serialized
                    // acceptance guards pass. A racing rejected send must not
                    // supersede the generation owned by the accepted Turn.
                    sync_generation = Some(self.history_sync.begin_send(&task_id));
                    // The tentative receipt closes the crash window before Chat writes. The
                    // second write proves those rows once existed even if authoritative Native
                    // Session replay later replaces their local message identities.
                    let mut receipt = TaskSendReceipt {
                        idempotency_key: idempotency_key.clone(),
                        text: fingerprint.text.clone(),
                        attachment_handles: fingerprint.attachment_handles.clone(),
                        user_message_id: user_message_id.as_str().to_string(),
                        turn_id: turn_id.as_str().to_string(),
                        durable_chat_written: false,
                    };
                    self.store.write_send_receipt(&task_id, receipt.clone())?;
                    self.append_user_message(
                        &task_id,
                        &idempotency_identity,
                        user_message_id.as_str(),
                        fingerprint.text.clone(),
                        attachment_reservation.attachments().chat_attachments(),
                        &now,
                    )?;
                    self.append_running_turn(&task_id, turn_id.as_str(), &now)?;
                    receipt.durable_chat_written = true;
                    self.store.write_send_receipt(&task_id, receipt)?;

                    let task = ctx.task_mut();
                    task.status = LegacyTaskStatus::Active;
                    // Promotion is durable before Agent work starts, so permissions and other
                    // Agent requests can never belong to a client-private New Task.
                    task.lifecycle = TaskLifecycle::Visible;
                    task.active_turn_id = Some(turn_id.as_str().to_string());
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                });
        let result = match commit_result {
            Ok(result) => result,
            Err(error) => {
                self.turn_acceptance
                    .retire_pending_turn(&task_id, turn_id.as_str());
                if let Some(generation) = sync_generation {
                    self.publish_idle_history_sync(&task_id, generation);
                }
                return Err(super::protocol_error_from_runtime(error));
            }
        };
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            self.turn_acceptance
                .retire_pending_turn(&task_id, turn_id.as_str());
            if let Some(generation) = sync_generation {
                self.publish_idle_history_sync(&task_id, generation);
            }
            return Err(self.stale_task_revision_error(&task_id)?);
        }
        let sync_generation = sync_generation.ok_or_else(|| {
            internal_error("Committed send did not acquire history sync ownership")
        })?;
        let attachments = attachment_reservation.commit();
        let committed_send =
            CommittedSend::new(task_id.clone(), turn_id.clone(), user_message_id.clone());
        self.publish_history_sync(
            &task_id,
            openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Syncing {
                generation: sync_generation,
            },
        );
        // Materialize the authoritative acceptance response before ACP work can
        // contend with session/storage resources. The Turn is already durable,
        // so the background start is launched regardless of snapshot success.
        let accepted = committed_send.accepted(self);

        let api = self.clone();
        let background_send = committed_send.clone();
        std::thread::spawn(move || {
            if let Err(error) = api.start_committed_send(
                existing_task,
                fingerprint.text,
                attachments,
                background_send,
                sync_generation,
            ) {
                crate::logging::error(
                    "task_committed_send_start_failed",
                    serde_json::json!({ "error": error.message }),
                );
            }
        });
        accepted
    }

    fn stale_task_revision_error(&self, task_id: &str) -> Result<ProtocolError, ProtocolError> {
        let snapshot = build_snapshot(&self.store, task_id, 100).map_err(storage_error)?;
        let current_task = self.project_task_snapshot(snapshot)?;
        Ok(ProtocolError {
            code: ProtocolErrorCode::Conflict,
            message: "Task changed before the message was sent".to_string(),
            recoverable: true,
            target: Some(ErrorTarget {
                method: None,
                field: Some("taskRevision".to_string()),
                current_task: Some(Box::new(current_task)),
            }),
        })
    }

    pub(super) fn start_committed_send(
        &self,
        existing_task: TaskRecord,
        prompt_text: String,
        attachments: ResolvedSendAttachments,
        committed_send: CommittedSend,
        sync_generation: u64,
    ) -> Result<(), ProtocolError> {
        let task_id = existing_task.task_id.clone();
        self.history_sync
            .run_send(&task_id.clone(), sync_generation, || {
                self.start_committed_send_exclusive(
                    existing_task,
                    prompt_text,
                    attachments,
                    committed_send,
                    sync_generation,
                )
            })
            .unwrap_or(Ok(()))
    }

    fn start_committed_send_exclusive(
        &self,
        existing_task: TaskRecord,
        prompt_text: String,
        attachments: ResolvedSendAttachments,
        committed_send: CommittedSend,
        sync_generation: u64,
    ) -> Result<(), ProtocolError> {
        let task_id = existing_task.task_id.clone();
        let turn_id = committed_send.turn_id().clone();
        let retryable_history_sync =
            existing_task.lifecycle.is_visible() && existing_task.agent_session_id.is_some();
        let pending_send = PendingSendSync {
            prompt_text: prompt_text.clone(),
            attachments: attachments.clone(),
            committed_send: committed_send.clone(),
        };

        let existing_task = match self.wait_for_session_preparation(existing_task, sync_generation)
        {
            Ok(Some(task)) => task,
            Ok(None) => return Ok(()),
            Err(error) => {
                self.handle_send_start_failure(
                    retryable_history_sync,
                    &task_id,
                    pending_send.clone(),
                    error,
                    "Could not prepare conversation history before sending",
                    sync_generation,
                )?;
                return Ok(());
            }
        };

        let opened_session = match self.open_agent_session(&existing_task) {
            Ok(opened_session) => opened_session,
            Err(error) => {
                let message = error.message.clone();
                self.handle_send_start_failure(
                    retryable_history_sync,
                    &task_id,
                    pending_send.clone(),
                    RuntimeError::Internal(error.message),
                    &message,
                    sync_generation,
                )?;
                return Ok(());
            }
        };
        let session_id = opened_session.session().session_id.clone();
        let session_task_state = opened_session.task_state();
        let replayed_messages = opened_session.replayed_messages().to_vec();
        let history_reconciled = !replayed_messages.is_empty();
        let reconciled_messages = if replayed_messages.is_empty() {
            None
        } else {
            let messages = self.store.read_messages(&task_id).map_err(runtime_error)?;
            let suffix_start = messages
                .iter()
                .position(|message| {
                    message.chat.message_id == committed_send.user_message_id().as_str()
                })
                .ok_or_else(|| {
                    internal_error("committed user message missing before history sync")
                })?;
            Some(
                replayed_messages
                    .into_iter()
                    .chain(
                        messages[suffix_start..]
                            .iter()
                            .map(|message| message.chat.message.clone()),
                    )
                    .collect::<Vec<_>>(),
            )
        };
        let session_commit =
            self.mutations
                .commit_existing_task(&task_id, durable_send_commit_options(), |ctx| {
                    if ctx.task().active_turn_id.as_deref() != Some(turn_id.as_str()) {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if let Some(messages) = reconciled_messages {
                        ctx.replace_messages(messages)?;
                    }
                    session_task_state.apply_to(ctx.task_mut());
                    Ok(TaskMutationResult::Changed)
                });
        match session_commit {
            Ok(result) if matches!(result.outcome, TaskCommitOutcome::Committed(_)) => {}
            Ok(_) => {
                self.handle_send_start_failure(
                    retryable_history_sync,
                    &task_id,
                    pending_send.clone(),
                    RuntimeError::NotReady("Conversation changed while synchronizing".to_string()),
                    "Conversation changed while synchronizing",
                    sync_generation,
                )?;
                return Ok(());
            }
            Err(error) => {
                self.handle_send_start_failure(
                    retryable_history_sync,
                    &task_id,
                    pending_send.clone(),
                    error,
                    "Could not save synchronized conversation history",
                    sync_generation,
                )?;
                return Ok(());
            }
        }
        // Bind the Native Session before attaching its metadata sink. Updates
        // emitted during session startup remain buffered until this point and
        // then pass the sink's stale-session guard.
        if let Err(error) = self
            .turn_runner
            .attach_session_events(task_id.clone(), &opened_session.session().key())
        {
            self.mutations
                .commit_existing_task(&task_id, durable_send_commit_options(), |ctx| {
                    if ctx.task().agent_session_id.as_deref() != Some(session_id.as_str()) {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    ctx.task_mut().agent_session_id = None;
                    Ok(TaskMutationResult::Changed)
                })
                .map_err(super::protocol_error_from_runtime)?;
            self.handle_send_start_failure(
                retryable_history_sync,
                &task_id,
                pending_send,
                error,
                "Could not attach the synchronized Agent session",
                sync_generation,
            )?;
            return Ok(());
        }
        let session = opened_session.commit();
        self.turn_runner.spawn_agent_turn(
            task_id.clone(),
            prompt_text,
            attachments.agent_attachments(),
            turn_id.as_str().to_string(),
            session,
        );
        self.turn_acceptance
            .retire_pending_turn(&task_id, turn_id.as_str());
        self.publish_history_sync(
            &task_id,
            if history_reconciled {
                openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Updated {
                    generation: sync_generation,
                }
            } else {
                openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle {
                    generation: sync_generation,
                }
            },
        );
        Ok(())
    }

    fn handle_send_start_failure(
        &self,
        retryable_history_sync: bool,
        task_id: &str,
        pending_send: PendingSendSync,
        error: RuntimeError,
        message: &str,
        sync_generation: u64,
    ) -> Result<(), ProtocolError> {
        self.turn_acceptance.serialize(task_id, || {
            if !self
                .history_sync
                .is_generation_current(task_id, sync_generation)
            {
                return Ok(());
            }
            if retryable_history_sync {
                self.pending_send_sync.defer(task_id, pending_send);
                self.publish_history_sync(
                    task_id,
                    openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Failed {
                        generation: sync_generation,
                        message: message.to_string(),
                        before_send: true,
                    },
                );
                return Ok(());
            }
            pending_send.committed_send.fail(self, error)?;
            self.turn_acceptance
                .retire_pending_turn(task_id, pending_send.committed_send.turn_id().as_str());
            self.publish_history_sync(
                task_id,
                openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle {
                    generation: sync_generation,
                },
            );
            Ok(())
        })
    }

    fn publish_idle_history_sync(&self, task_id: &str, generation: u64) {
        self.publish_history_sync(
            task_id,
            openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot::Idle { generation },
        );
    }

    fn wait_for_session_preparation(
        &self,
        initial: TaskRecord,
        sync_generation: u64,
    ) -> Result<Option<TaskRecord>, RuntimeError> {
        let task_id = initial.task_id.clone();
        let lifecycle_before_send = initial.lifecycle.clone();
        let mut initial = (!matches!(
            initial.preparation,
            TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing
        ))
        .then_some(initial);
        self.history_sync
            .wait_for_current_send(&task_id, sync_generation, || {
                let mut task = match initial.take() {
                    Some(task) => task,
                    None => self.store.read_task(&task_id)?,
                };
                // The persisted Task already includes this accepted send. Session opening still
                // needs the pre-send value to distinguish a draft from recoverable Agent history.
                task.lifecycle = lifecycle_before_send.clone();
                match &task.preparation {
                    TaskPreparationRecord::Ready => Ok(Some(task)),
                    TaskPreparationRecord::Failed { message } => Err(RuntimeError::NotReady(
                        format!("Task Agent preparation failed: {message}"),
                    )),
                    TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing => Ok(None),
                }
            })
    }

    fn recover_stale_active_turn(&self, task_id: &str, turn_id: &str) -> Result<(), ProtocolError> {
        TaskTransitions::new(self.mutations.clone())
            .cancel_running_task(task_id, Some(turn_id), RESTART_INTERRUPTION_MESSAGE, true)
            .map_err(super::protocol_error_from_runtime)?;
        Ok(())
    }
}

fn durable_send_commit_options() -> super::TaskCommitOptions {
    super::TaskCommitOptions {
        refresh_message_history: true,
        response_snapshot_tail_limit: None,
    }
}
