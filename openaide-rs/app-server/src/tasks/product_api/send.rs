use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{ClientInstanceId, MessageId, TurnId};
use openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot;
use openaide_app_server_protocol::task::{TaskQueueSendSelection, TaskSendParams};
use std::time::Instant;
use uuid::Uuid;

use crate::attachment_runtime::{AttachmentSendReservation, ResolvedSendAttachments};
use crate::projects::ProjectIdentity;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus as LegacyTaskStatus;
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::tasks::native_session_service::PrimaryPromptRequest;
use crate::tasks::transitions::TaskTransitions;
use crate::time::now_string;

use super::{
    conflict_error, runtime_error, storage_error, validation_error, TaskProductApi,
    TaskSendAccepted,
};
pub(crate) mod committed;
pub(super) mod support;

use committed::CommittedSend;
use support::{normalized_message_text, prompt_title, protocol_error_from_attachment_runtime};

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
        let mut existing_task =
            self.read_interactive_task_for_client(&task_id, client_instance_id)?;
        if existing_task.active_turn_id.is_none() {
            self.turn_acceptance.retire_for_idle_task(&task_id);
        }
        if !std::path::Path::new(&existing_task.workspace_root).is_dir() {
            return Err(conflict_error(
                "Task workspace is unavailable. Restore it before sending.",
            ));
        }
        super::prepare::reject_if_preparation_not_ready(&existing_task)?;
        if let Some(active_turn_id) = existing_task.active_turn_id.clone() {
            let active_turn_is_live = self
                .turn_runner
                .active_turn_is_live(&task_id, active_turn_id.as_str())
                || self
                    .turn_acceptance
                    .owns_pending_turn(&task_id, active_turn_id.as_str());
            if !active_turn_is_live {
                self.recover_stale_active_turn(&task_id, active_turn_id.as_str())?;
                existing_task = self.store.read_task(&task_id).map_err(runtime_error)?;
            }
        }
        let steering_turn_id = existing_task
            .active_turn_id
            .clone()
            .filter(|_| matches!(existing_task.status, LegacyTaskStatus::Active));
        if existing_task.active_turn_id.is_some() && steering_turn_id.is_none() {
            return Err(conflict_error("Task is not ready to accept steering"));
        }
        if params.message.attachments.len() > 20 {
            return Err(validation_error(
                "message.attachments",
                "A draft can attach at most 20 files",
            ));
        }
        let owner =
            crate::attachment_runtime::AttachmentOwner::new(client_instance_id, &params.task_id);
        let attachment_reservation = self
            .attachments
            .reserve_for_send(&owner, &params.message.attachments)
            .map_err(protocol_error_from_attachment_runtime)?;
        let mut attachments = attachment_reservation
            .resolved_with_inline_images(&params.message.images)
            .map_err(protocol_error_from_attachment_runtime)?;
        let prompt_text = normalized_message_text(&params.message);
        let queue_selection = params.queue_selection.clone();
        if let Some(selection) = queue_selection.as_ref() {
            if !params.message.images.is_empty() || !params.message.attachments.is_empty() {
                return Err(validation_error(
                    "message.attachments",
                    "Send now uses the queued message's attachments",
                ));
            }
            if existing_task.message_queue.revision != selection.queue_revision {
                return Err(conflict_error("Task Message Queue changed"));
            }
            let queued = existing_task
                .message_queue
                .items
                .iter()
                .find(|item| item.queued_message_id == selection.queued_message_id.as_str())
                .ok_or_else(|| conflict_error("Queued Message no longer exists"))?;
            if queued.text != prompt_text {
                return Err(conflict_error("Queued Message changed"));
            }
            attachments = self.resolved_queued_attachments(&existing_task, queued)?;
        }
        if prompt_text.is_empty() && attachments.is_empty() {
            return Err(validation_error("message.text", "Message text is required"));
        }
        if attachments.contains_images() && !existing_task.supports_image_input {
            return Err(ProtocolError {
                code:
                    openaide_app_server_protocol::errors::ProtocolErrorCode::CapabilityUnavailable,
                message: "The selected Agent does not accept Images".to_string(),
                recoverable: true,
                target: None,
            });
        }
        self.agent_registry
            .require(&existing_task.agent_id)
            .map_err(super::protocol_error_from_runtime)?;
        let now = now_string();
        let user_message_id = MessageId::from(format!("message_{}", Uuid::new_v4()));
        if let Some(turn_id) = steering_turn_id {
            return self.accept_steering_message(
                client_instance_id,
                existing_task,
                TurnId::from(turn_id),
                user_message_id,
                prompt_text,
                attachments,
                attachment_reservation,
                now,
                queue_selection,
            );
        }

        let turn_id = TurnId::from(format!("turn_{}", Uuid::new_v4()));
        if !self
            .turn_acceptance
            .own_pending_turn(&task_id, turn_id.as_str())
        {
            return Err(conflict_error("Task is already starting a Turn"));
        }
        let sending_client = client_instance_id.clone();
        let mut promoted_new_task = false;
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
                    if super::prepare::reject_if_preparation_not_ready(ctx.task()).is_err() {
                        return Ok(TaskMutationResult::Rejected);
                    }
                    promoted_new_task =
                        matches!(ctx.task().lifecycle, TaskLifecycle::Prepared { .. });
                    let queue_revision_before = ctx.task().message_queue.revision;
                    consume_selected_queue_message(
                        ctx.task_mut(),
                        queue_selection.as_ref(),
                        &prompt_text,
                    )?;
                    clear_queue_pause(ctx.task_mut(), queue_revision_before);
                    self.append_user_message(
                        ctx,
                        &format!("user:{}", user_message_id.as_str()),
                        user_message_id.as_str(),
                        prompt_text.clone(),
                        attachments.chat_attachments(),
                        &now,
                    )?;
                    self.append_running_turn(ctx, &task_id, turn_id.as_str(), &now)?;

                    let task = ctx.task_mut();
                    record_composer_history(task, user_message_id.as_str(), &prompt_text, &now);
                    task.status = LegacyTaskStatus::Starting;
                    // Promotion is durable before Agent work starts, so permissions and other
                    // Agent requests can never belong to a client-private New Task.
                    task.lifecycle = TaskLifecycle::Open;
                    if promoted_new_task && task.title.is_empty() {
                        task.title.set_prompt_title(prompt_title(&prompt_text));
                    }
                    task.active_turn_id = Some(turn_id.as_str().to_string());
                    task.active_turn_started_at = Some(now.clone());
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
        let committed_task = match result.outcome {
            TaskCommitOutcome::Committed(facts) => facts.committed_task,
            TaskCommitOutcome::Rejected(_) => {
                self.turn_acceptance
                    .retire_pending_turn(&task_id, turn_id.as_str());
                let current = self.read_task_for_client(&task_id, client_instance_id)?;
                super::prepare::reject_if_preparation_not_ready(&current)?;
                return Err(conflict_error("Task is already running"));
            }
        };
        self.native_sessions.user_message_accepted(&task_id);
        // Handles remain retryable until the user message is durably accepted.
        let attachments = attachment_reservation.commit_with(attachments);
        if promoted_new_task {
            let project = ProjectIdentity::from_workspace_root(&committed_task.workspace_root);
            let defaults = NewTaskDefaultsSnapshot {
                project_id: Some(project.project_id),
                agent_id: Some(committed_task.agent_id.clone().into()),
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
        let committed_send =
            CommittedSend::new(task_id.clone(), turn_id.clone(), user_message_id.clone());
        // Materialize the authoritative acceptance response before ACP work can
        // contend with session/storage resources. The Turn is already durable,
        // so the background start is launched regardless of snapshot success.
        let accepted = committed_send.accepted(self);

        let background_started_at = Instant::now();
        let background_task_id = committed_task.task_id.clone();
        let background_turn_id = turn_id.clone();
        crate::logging::info(
            "task_primary_prompt_background_start_scheduled",
            serde_json::json!({
                "task_id": background_task_id.as_str(),
                "turn_id": background_turn_id.as_str(),
            }),
        );
        let api = self.clone();
        let background_send = committed_send.clone();
        std::thread::spawn(move || {
            let worker_started_at = Instant::now();
            crate::logging::info(
                "task_primary_prompt_background_start_entered",
                serde_json::json!({
                    "task_id": background_task_id.as_str(),
                    "turn_id": background_turn_id.as_str(),
                    "scheduled_to_entry_ms": background_started_at.elapsed().as_millis(),
                }),
            );
            let result =
                api.start_committed_send(committed_task, prompt_text, attachments, background_send);
            crate::logging::info(
                "task_primary_prompt_background_start_returned",
                serde_json::json!({
                    "task_id": background_task_id.as_str(),
                    "turn_id": background_turn_id.as_str(),
                    "outcome": if result.is_ok() { "ok" } else { "error" },
                    "worker_elapsed_ms": worker_started_at.elapsed().as_millis(),
                }),
            );
            if let Err(error) = result {
                crate::logging::error(
                    "task_committed_send_start_failed",
                    serde_json::json!({
                        "task_id": background_task_id.as_str(),
                        "turn_id": background_turn_id.as_str(),
                        "error": error.message,
                    }),
                );
            }
        });
        crate::logging::info(
            "task_primary_prompt_background_start_spawned",
            serde_json::json!({
                "task_id": task_id.as_str(),
                "turn_id": turn_id.as_str(),
                "scheduled_to_spawned_ms": background_started_at.elapsed().as_millis(),
            }),
        );
        accepted
    }

    #[allow(clippy::too_many_arguments)]
    fn accept_steering_message(
        &self,
        client_instance_id: &ClientInstanceId,
        existing_task: TaskRecord,
        active_turn_id: TurnId,
        user_message_id: MessageId,
        prompt_text: String,
        attachments: ResolvedSendAttachments,
        attachment_reservation: AttachmentSendReservation,
        now: String,
        queue_selection: Option<TaskQueueSendSelection>,
    ) -> Result<TaskSendAccepted, ProtocolError> {
        let task_id = existing_task.task_id.clone();
        let sending_client = client_instance_id.clone();
        let expected_session_id = existing_task.agent_session_id.clone();
        let result = self
            .mutations
            .commit_existing_task(&task_id, durable_send_commit_options(), |ctx| {
                if ctx.task().tombstoned
                    || crate::tasks::access::require_client_task_access(ctx.task(), &sending_client)
                        .is_err()
                    || ctx.task().status != LegacyTaskStatus::Active
                    || ctx.task().active_turn_id.as_deref() != Some(active_turn_id.as_str())
                    || ctx.task().agent_session_id != expected_session_id
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                let queue_revision_before = ctx.task().message_queue.revision;
                consume_selected_queue_message(
                    ctx.task_mut(),
                    queue_selection.as_ref(),
                    &prompt_text,
                )?;
                clear_queue_pause(ctx.task_mut(), queue_revision_before);
                self.append_user_message(
                    ctx,
                    &format!("user:{}", user_message_id.as_str()),
                    user_message_id.as_str(),
                    prompt_text.clone(),
                    attachments.chat_attachments(),
                    &now,
                )?;
                let task = ctx.task_mut();
                record_composer_history(task, user_message_id.as_str(), &prompt_text, &now);
                task.updated_at = now.clone();
                task.last_activity = now.clone();
                Ok(TaskMutationResult::Changed)
            })
            .map_err(super::protocol_error_from_runtime)?;
        let committed_task = match result.outcome {
            TaskCommitOutcome::Committed(facts) => facts.committed_task,
            TaskCommitOutcome::Rejected(_) => {
                return Err(conflict_error("Task is no longer accepting steering"));
            }
        };
        self.native_sessions.user_message_accepted(&task_id);
        // Handles remain retryable until the steering message is durably accepted.
        let attachments = attachment_reservation.commit_with(attachments);
        let snapshot = crate::tasks::snapshot::build_snapshot(&self.store, &task_id, 100)
            .map_err(storage_error)?;
        let accepted = TaskSendAccepted {
            task: self.project_task_snapshot(snapshot)?,
            turn_id: active_turn_id,
            user_message_id,
        };

        // Register the ACP request before releasing Task send serialization. Otherwise
        // primary settlement can overtake this accepted steer and orphan its response.
        if let Err(error) =
            self.native_sessions
                .steer(committed_task, prompt_text, attachments.agent_attachments())
        {
            crate::logging::error(
                "task_steering_prompt_failed",
                serde_json::json!({ "task_id": task_id, "error": error.to_string() }),
            );
        }
        Ok(accepted)
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
        crate::logging::info(
            "task_primary_prompt_start_started",
            serde_json::json!({
                "task_id": task_id.as_str(),
                "turn_id": turn_id.as_str(),
            }),
        );
        let result = match self
            .native_sessions
            .start_primary_prompt(PrimaryPromptRequest {
                task: existing_task,
                turn_id: turn_id.clone(),
                text: prompt_text,
                attachments: attachments.agent_attachments(),
            }) {
            Ok(()) => Ok(()),
            Err(error) => committed_send.fail(self, error).map(|_| ()),
        };
        crate::logging::info(
            "task_primary_prompt_start_completed",
            serde_json::json!({
                "task_id": task_id.as_str(),
                "turn_id": turn_id.as_str(),
                "outcome": if result.is_ok() { "started" } else { "failed" },
            }),
        );
        // Ownership must retire even when recovery itself reports a storage error;
        // otherwise every later Send is rejected by stale in-memory admission state.
        self.turn_acceptance
            .retire_pending_turn(&task_id, turn_id.as_str());
        result
    }

    fn recover_stale_active_turn(&self, task_id: &str, turn_id: &str) -> Result<(), ProtocolError> {
        TaskTransitions::new(self.mutations.clone(), self.server_requests.clone())
            .end_active_work(
                task_id,
                Some(turn_id),
                crate::tasks::transitions::ActiveWorkEnd::Restarted,
            )
            .map_err(super::protocol_error_from_runtime)?;
        Ok(())
    }
}

fn consume_selected_queue_message(
    task: &mut TaskRecord,
    selection: Option<&TaskQueueSendSelection>,
    prompt_text: &str,
) -> Result<(), RuntimeError> {
    let Some(selection) = selection else {
        return Ok(());
    };
    if task.message_queue.revision != selection.queue_revision {
        return Err(RuntimeError::Conflict(
            "Task Message Queue changed".to_string(),
        ));
    }
    let index = task
        .message_queue
        .items
        .iter()
        .position(|item| item.queued_message_id == selection.queued_message_id.as_str())
        .ok_or_else(|| RuntimeError::Conflict("Queued Message no longer exists".to_string()))?;
    if task.message_queue.items[index].text != prompt_text {
        return Err(RuntimeError::Conflict("Queued Message changed".to_string()));
    }
    task.message_queue.items.remove(index);
    task.message_queue.revision = task.message_queue.revision.saturating_add(1);
    Ok(())
}

fn clear_queue_pause(task: &mut TaskRecord, revision_before: u64) {
    if task.message_queue.pause.take().is_some() && task.message_queue.revision == revision_before {
        task.message_queue.revision = task.message_queue.revision.saturating_add(1);
    }
}

pub(super) fn record_composer_history(
    task: &mut TaskRecord,
    entry_id: &str,
    text: &str,
    accepted_at: &str,
) {
    if text.is_empty() {
        return;
    }
    let project_id = crate::projects::project_id_for_workspace(
        task.project_root
            .as_deref()
            .unwrap_or(task.workspace_root.as_str()),
    );
    task.composer_history.record(
        crate::storage::composer_history::ComposerHistoryEntryRecord {
            entry_id: entry_id.to_string(),
            project_id: project_id.as_str().to_string(),
            text: text.to_string(),
            accepted_at: accepted_at.to_string(),
        },
    );
}

fn durable_send_commit_options() -> super::TaskCommitOptions {
    super::TaskCommitOptions {
        refresh_message_history: true,
        response_snapshot_tail_limit: None,
    }
}
