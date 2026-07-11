use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{ClientInstanceId, MessageId, TurnId};
use openaide_app_server_protocol::task::TaskSendParams;
use uuid::Uuid;

use crate::attachment_runtime::{AttachmentOwner, AttachmentSendReservation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::snapshots::task_snapshot::project_stored_task_snapshot;
use crate::storage::send_receipts::TaskSendReceipt;
use crate::task_recovery::RESTART_INTERRUPTION_MESSAGE;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::tasks::snapshot::build_snapshot;
use crate::tasks::transitions::TaskTransitions;
use crate::time::now_string;

use super::{
    conflict_error, runtime_error, storage_error, validation_error, TaskProductApi,
    TaskSendAccepted,
};

mod session;

use revision_guard::same_send_target;

mod committed;
mod recovery;
mod revision_guard;
mod support;

use committed::CommittedSend;
use support::{
    protocol_error_from_attachment_runtime, send_identity, title_from_prompt, SendFingerprint,
};

impl TaskProductApi {
    pub(super) fn send_message(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskSendParams,
    ) -> Result<TaskSendAccepted, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let mut existing_task = self.store.read_task(&task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&existing_task)?;
        let idempotency_key = params.idempotency_key.as_str().to_string();
        let idempotency_identity = send_identity(&idempotency_key);
        if let Some(existing) = self.existing_send(&task_id, &idempotency_key)? {
            existing.validate(&SendFingerprint::from_request_message(&params.message))?;
            self.recover_existing_send(&task_id, &existing)?;
            let snapshot = build_snapshot(&self.store, &task_id, 100).map_err(storage_error)?;
            let task = project_stored_task_snapshot(snapshot)?;
            return Ok(TaskSendAccepted {
                task,
                turn_id: existing.turn_id,
                user_message_id: existing.user_message_id,
            });
        }
        let attachment_owner = AttachmentOwner::new(client_instance_id, &params.task_id);
        let attachment_reservation = self
            .attachments
            .reserve_for_send(&attachment_owner, &params.message.attachments)
            .map_err(protocol_error_from_attachment_runtime)?;
        let fingerprint =
            SendFingerprint::from_message(&params.message, attachment_reservation.attachments())?;
        let recovered_stale_turn =
            if let Some(active_turn_id) = existing_task.active_turn_id.clone() {
                if self
                    .turn_runner
                    .active_turn_is_live(&task_id, active_turn_id.as_str())
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
            if existing_task.status == LegacyTaskStatus::Blocked {
                return Err(conflict_error(
                    "Resolve the pending permission request before sending another message",
                ));
            }
            return self.send_steering_message(
                existing_task,
                idempotency_key,
                idempotency_identity,
                fingerprint,
                attachment_reservation,
            );
        }
        if !recovered_stale_turn && existing_task.revision != params.task_revision {
            return Err(conflict_error("Task changed before the message was sent"));
        }
        super::prepare::reject_if_preparation_not_ready(&existing_task)?;
        self.agent_registry
            .require(&existing_task.agent_id)
            .map_err(super::protocol_error_from_runtime)?;
        let now = now_string();
        let turn_id = TurnId::from(format!("turn_{}", Uuid::new_v4()));
        let user_message_id = MessageId::from(format!("message_{}", Uuid::new_v4()));
        let commit_result =
            self.mutations
                .commit_existing_task(&task_id, durable_send_commit_options(), |ctx| {
                    if ctx.task().tombstoned {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if ctx.task().active_turn_id.is_some() {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if !same_send_target(&existing_task, ctx.task()) {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if super::prepare::reject_if_preparation_not_ready(ctx.task()).is_err() {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    self.store.write_send_receipt(
                        &task_id,
                        TaskSendReceipt {
                            idempotency_key: idempotency_key.clone(),
                            text: fingerprint.text.clone(),
                            attachment_handles: fingerprint.attachment_handles.clone(),
                            user_message_id: user_message_id.as_str().to_string(),
                            turn_id: turn_id.as_str().to_string(),
                        },
                    )?;
                    self.append_user_message(
                        &task_id,
                        &idempotency_identity,
                        user_message_id.as_str(),
                        fingerprint.text.clone(),
                        attachment_reservation.attachments().chat_attachments(),
                        &now,
                    )?;
                    self.append_running_turn(&task_id, turn_id.as_str(), &now)?;

                    let task = ctx.task_mut();
                    task.status = LegacyTaskStatus::Active;
                    task.first_prompt_sent = true;
                    task.active_turn_id = Some(turn_id.as_str().to_string());
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    if task.title == "New task" {
                        task.title = title_from_prompt(&fingerprint.text);
                    }
                    Ok(TaskMutationResult::Changed)
                });
        let result = commit_result.map_err(super::protocol_error_from_runtime)?;
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(conflict_error("Task changed before the message was sent"));
        }
        let attachments = attachment_reservation.commit();
        let committed_send =
            CommittedSend::new(task_id.clone(), turn_id.clone(), user_message_id.clone());

        let opened_session = match self.open_agent_session(&existing_task) {
            Ok(opened_session) => opened_session,
            Err(error) => {
                return committed_send.fail_protocol(self, &error);
            }
        };
        let session_id = opened_session.session().session_id.clone();
        let session_config_options = opened_session.session().config_options.clone();
        let session_config_catalog = opened_session.session().config_catalog.clone();
        let session_model_id = opened_session.session().model_id.clone();
        let session_commit =
            self.mutations
                .commit_existing_task(&task_id, durable_send_commit_options(), |ctx| {
                    if ctx.task().active_turn_id.as_deref() != Some(turn_id.as_str()) {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    let task = ctx.task_mut();
                    task.agent_session_id = Some(session_id.clone());
                    for (config_id, value) in &session_config_options {
                        task.config_options
                            .entry(config_id.clone())
                            .or_insert_with(|| value.clone());
                    }
                    if task.config_options_catalog.is_none() {
                        task.config_options_catalog = session_config_catalog.clone();
                    }
                    task.model_id = task.model_id.clone().or(session_model_id.clone());
                    Ok(TaskMutationResult::Changed)
                });
        match session_commit {
            Ok(result) if matches!(result.outcome, TaskCommitOutcome::Committed(_)) => {}
            Ok(_) => {
                let error = RuntimeError::NotReady(
                    "Task changed before session start completed".to_string(),
                );
                return committed_send.fail(self, error);
            }
            Err(error) => {
                return committed_send.fail(self, error);
            }
        }
        // Bind the Native Session before attaching its metadata sink. Updates
        // emitted during session startup remain buffered until this point and
        // then pass the sink's stale-session guard.
        if let Err(error) = self
            .turn_runner
            .attach_session_events(task_id.clone(), &session_id)
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
            return committed_send.fail(self, error);
        }
        let session = opened_session.commit();
        self.turn_runner.spawn_agent_turn(
            task_id,
            fingerprint.text.clone(),
            attachments.agent_attachments(),
            turn_id.as_str().to_string(),
            session,
        );

        committed_send.accepted(self)
    }

    fn send_steering_message(
        &self,
        existing_task: crate::storage::records::TaskRecord,
        idempotency_key: String,
        idempotency_identity: String,
        fingerprint: SendFingerprint,
        attachment_reservation: AttachmentSendReservation,
    ) -> Result<TaskSendAccepted, ProtocolError> {
        let Some(active_turn_id) = existing_task.active_turn_id.clone() else {
            return Err(conflict_error("Task does not have an active turn"));
        };
        let Some(agent_session_id) = existing_task.agent_session_id.clone() else {
            return Err(conflict_error(
                "Task active turn does not have a live session",
            ));
        };
        self.agent_registry
            .require(&existing_task.agent_id)
            .map_err(super::protocol_error_from_runtime)?;
        let task_id = existing_task.task_id.clone();
        let now = now_string();
        let turn_id = TurnId::from(active_turn_id.clone());
        let user_message_id = MessageId::from(format!("message_{}", Uuid::new_v4()));
        let steering = self
            .turn_runner
            .reserve_steering(&task_id, &active_turn_id)
            .map_err(super::protocol_error_from_runtime)?;
        let commit_result =
            self.mutations
                .commit_existing_task(&task_id, durable_send_commit_options(), |ctx| {
                    if ctx.task().tombstoned {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    if ctx.task().active_turn_id.as_deref() != Some(active_turn_id.as_str())
                        || ctx.task().agent_session_id.as_deref() != Some(agent_session_id.as_str())
                    {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    self.store.write_send_receipt(
                        &task_id,
                        TaskSendReceipt {
                            idempotency_key: idempotency_key.clone(),
                            text: fingerprint.text.clone(),
                            attachment_handles: fingerprint.attachment_handles.clone(),
                            user_message_id: user_message_id.as_str().to_string(),
                            turn_id: turn_id.as_str().to_string(),
                        },
                    )?;
                    self.append_user_message(
                        &task_id,
                        &idempotency_identity,
                        user_message_id.as_str(),
                        fingerprint.text.clone(),
                        attachment_reservation.attachments().chat_attachments(),
                        &now,
                    )?;

                    let task = ctx.task_mut();
                    task.status = LegacyTaskStatus::Active;
                    task.first_prompt_sent = true;
                    task.updated_at = now.clone();
                    task.last_activity = now;
                    Ok(TaskMutationResult::Changed)
                });
        let result = commit_result.map_err(super::protocol_error_from_runtime)?;
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(conflict_error("Task changed before the message was sent"));
        }
        let attachments = attachment_reservation.commit();
        let committed_send =
            CommittedSend::new(task_id.clone(), turn_id.clone(), user_message_id.clone());
        if let Err(error) =
            steering.dispatch(fingerprint.text.clone(), attachments.agent_attachments())
        {
            return committed_send.fail(self, error);
        }

        committed_send.accepted(self)
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
