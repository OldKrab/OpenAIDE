use openaide_app_server_protocol::errors::{ErrorTarget, ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, MessageId, TurnId};
use openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot;
use openaide_app_server_protocol::task::TaskSendParams;
use uuid::Uuid;

use crate::attachment_runtime::{AttachmentOwner, ResolvedSendAttachments};
use crate::projects::ProjectIdentity;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::storage::records::{TaskLifecycle, TaskRecord};
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

pub(crate) mod committed;
mod revision_guard;
mod support;

use committed::CommittedSend;
use support::{normalized_message_text, protocol_error_from_attachment_runtime};

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
        super::prepare::reject_if_preparation_not_ready(&existing_task)?;
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
        let prompt_text = normalized_message_text(&params.message);
        if prompt_text.is_empty()
            && attachment_reservation
                .attachments()
                .fingerprint_handles()
                .is_empty()
        {
            return Err(validation_error("message.text", "Message text is required"));
        }
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
                    self.append_user_message(
                        &task_id,
                        &format!("user:{}", user_message_id.as_str()),
                        user_message_id.as_str(),
                        prompt_text.clone(),
                        attachment_reservation.attachments().chat_attachments(),
                        &now,
                    )?;
                    self.append_running_turn(&task_id, turn_id.as_str(), &now)?;

                    let task = ctx.task_mut();
                    task.status = LegacyTaskStatus::Starting;
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
                return Err(super::protocol_error_from_runtime(error));
            }
        };
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            self.turn_acceptance
                .retire_pending_turn(&task_id, turn_id.as_str());
            return Err(self.stale_task_revision_error(&task_id)?);
        }
        if matches!(existing_task.lifecycle, TaskLifecycle::New { .. }) {
            let project = ProjectIdentity::from_workspace_root(&existing_task.workspace_root);
            let defaults = NewTaskDefaultsSnapshot {
                project_id: Some(project.project_id),
                agent_id: Some(existing_task.agent_id.clone().into()),
            };
            // Defaults are auxiliary initialization state. A preference write failure must not
            // invalidate a user message that is already durably accepted.
            if let Err(error) = self.store.write_new_task_defaults(&defaults) {
                crate::logging::error(
                    "new_task_defaults_write_failed",
                    serde_json::json!({ "task_id": task_id, "error": error.to_string() }),
                );
            }
        }
        let attachments = attachment_reservation.commit();
        let committed_send =
            CommittedSend::new(task_id.clone(), turn_id.clone(), user_message_id.clone());
        // Materialize the authoritative acceptance response before ACP work can
        // contend with session/storage resources. The Turn is already durable,
        // so the background start is launched regardless of snapshot success.
        let accepted = committed_send.accepted(self);

        let api = self.clone();
        let background_send = committed_send.clone();
        std::thread::spawn(move || {
            if let Err(error) =
                api.start_committed_send(existing_task, prompt_text, attachments, background_send)
            {
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
    ) -> Result<(), ProtocolError> {
        let task_id = existing_task.task_id.clone();
        let turn_id = committed_send.turn_id().clone();

        let opened_session = match self.open_agent_session(&existing_task) {
            Ok(opened_session) => opened_session,
            Err(error) => {
                committed_send.fail(self, RuntimeError::Internal(error.message))?;
                self.turn_acceptance
                    .retire_pending_turn(&task_id, turn_id.as_str());
                return Ok(());
            }
        };
        let session_id = opened_session.session().session_id.clone();
        let session_task_state = opened_session.task_state();
        let session_commit =
            self.mutations
                .commit_existing_task(&task_id, durable_send_commit_options(), |ctx| {
                    if ctx.task().active_turn_id.as_deref() != Some(turn_id.as_str()) {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    session_task_state.apply_to(ctx.task_mut());
                    Ok(TaskMutationResult::Changed)
                });
        match session_commit {
            Ok(result) if matches!(result.outcome, TaskCommitOutcome::Committed(_)) => {}
            Ok(_) => {
                committed_send.fail(
                    self,
                    RuntimeError::NotReady(
                        "Native Session changed before prompt start".to_string(),
                    ),
                )?;
                self.turn_acceptance
                    .retire_pending_turn(&task_id, turn_id.as_str());
                return Ok(());
            }
            Err(error) => {
                committed_send.fail(self, error)?;
                self.turn_acceptance
                    .retire_pending_turn(&task_id, turn_id.as_str());
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
            committed_send.fail(self, error)?;
            self.turn_acceptance
                .retire_pending_turn(&task_id, turn_id.as_str());
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
        Ok(())
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
