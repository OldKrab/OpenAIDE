use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::{
    TakenQueuedMessage, TaskQueueAppendParams, TaskQueueMoveParams, TaskQueueRemoveParams,
    TaskQueueTakeParams, TaskQueueTakeResult,
};
use uuid::Uuid;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus;
use crate::storage::records::QueuedMessageRecord;
use crate::tasks::mutation::TaskMutationResult;
use crate::time::now_string;

use super::{
    conflict_error, internal_error, protocol_error_from_runtime, validation_error, TaskProductApi,
};

const MAX_QUEUE_ITEMS: usize = 20;

impl TaskProductApi {
    /// Accepts queued text under the same ordering boundary as Send and Turn settlement.
    pub(super) fn queue_append_message(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskQueueAppendParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.turn_acceptance.serialize(&task_id, || {
            self.queue_append_message_serialized(client_instance_id, params)
        })
    }

    fn queue_append_message_serialized(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskQueueAppendParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.read_interactive_task_for_client(params.task_id.as_str(), client_instance_id)?;
        if params.message.attachments.len() > 20 {
            return Err(validation_error(
                "message.attachments",
                "A queued message can attach at most 20 files",
            ));
        }
        let owner =
            crate::attachment_runtime::AttachmentOwner::new(client_instance_id, &params.task_id);
        let attachment_reservation = self
            .attachments
            .reserve_for_send(&owner, &params.message.attachments)
            .map_err(super::send::support::protocol_error_from_attachment_runtime)?;
        let attachments = attachment_reservation
            .resolved_with_inline_images(&params.message.images)
            .map_err(super::send::support::protocol_error_from_attachment_runtime)?;
        let text = params
            .message
            .text
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        if text.is_empty() && attachments.is_empty() {
            return Err(validation_error("message.text", "Message text is required"));
        }
        let chat_attachments = attachments.chat_attachments();
        let agent_attachments = attachments.agent_attachments();
        let now = now_string();
        let queued_message_id = format!("queued_message_{}", Uuid::new_v4());
        let sending_client = client_instance_id.clone();
        let result = self
            .mutations
            .commit_existing_task(
                params.task_id.as_str(),
                super::response_snapshot_options(),
                |ctx| {
                    crate::tasks::access::require_client_task_access(ctx.task(), &sending_client)?;
                    if !matches!(
                        ctx.task().status,
                        TaskStatus::Starting
                            | TaskStatus::Active
                            | TaskStatus::Waiting
                            | TaskStatus::Stopping
                    ) {
                        return Err(RuntimeError::Conflict(
                            "Queue is available only while Agent work is active".to_string(),
                        ));
                    }
                    if ctx.task().message_queue.items.len() >= MAX_QUEUE_ITEMS {
                        return Err(RuntimeError::Conflict(
                            "A Task Message Queue can contain at most 20 items".to_string(),
                        ));
                    }
                    let task = ctx.task_mut();
                    task.message_queue.items.push(QueuedMessageRecord {
                        queued_message_id: queued_message_id.clone(),
                        text: text.clone(),
                        created_at: now.clone(),
                        chat_attachments: chat_attachments.clone(),
                        agent_attachments: agent_attachments.clone(),
                    });
                    super::send::record_composer_history(task, &queued_message_id, &text, &now);
                    task.message_queue.revision = task.message_queue.revision.saturating_add(1);
                    task.updated_at = now.clone();
                    Ok(TaskMutationResult::Changed)
                },
            )
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing queue append snapshot"))?;
        // Client-owned handles become queue-owned only after the Task mutation is durable.
        let _persisted_attachments = attachment_reservation.commit_with(attachments);
        crate::logging::info(
            "task_queue_message_appended",
            serde_json::json!({
                "task_id": params.task_id.as_str(),
                "queued_message_id": queued_message_id,
                "queue_revision": snapshot.message_queue.revision,
                "queue_length": snapshot.message_queue.items.len(),
            }),
        );
        self.project_task_snapshot(snapshot)
    }

    /// Revalidates live file references before queued delivery and restores durable payloads.
    pub(super) fn resolved_queued_attachments(
        &self,
        task: &crate::storage::records::TaskRecord,
        queued: &QueuedMessageRecord,
    ) -> Result<crate::attachment_runtime::ResolvedSendAttachments, ProtocolError> {
        validate_queued_attachment_paths(task, &queued.agent_attachments)?;
        Ok(
            crate::attachment_runtime::ResolvedSendAttachments::from_persisted(
                queued.chat_attachments.clone(),
                queued.agent_attachments.clone(),
            ),
        )
    }

    /// Removes exactly one observed queued item; there is intentionally no confirmation state.
    pub(super) fn queue_remove_message(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskQueueRemoveParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.turn_acceptance
            .serialize(&task_id, || {
                self.read_interactive_task_for_client(&task_id, client_instance_id)?;
                let sending_client = client_instance_id.clone();
                let result = self
                    .mutations
                    .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                        crate::tasks::access::require_client_task_access(
                            ctx.task(),
                            &sending_client,
                        )?;
                        if ctx.task().message_queue.revision != params.queue_revision {
                            return Err(RuntimeError::Conflict(
                                "Task Message Queue changed".to_string(),
                            ));
                        }
                        let index = ctx
                            .task()
                            .message_queue
                            .items
                            .iter()
                            .position(|item| {
                                item.queued_message_id == params.queued_message_id.as_str()
                            })
                            .ok_or_else(|| {
                                RuntimeError::Conflict(
                                    "Queued Message no longer exists".to_string(),
                                )
                            })?;
                        let task = ctx.task_mut();
                        task.message_queue.items.remove(index);
                        if task.message_queue.items.is_empty() {
                            task.message_queue.pause = None;
                        }
                        task.message_queue.revision = task.message_queue.revision.saturating_add(1);
                        task.updated_at = now_string();
                        Ok(TaskMutationResult::Changed)
                    })
                    .map_err(protocol_error_from_runtime)?;
                let snapshot = result
                    .response_snapshot
                    .ok_or_else(|| internal_error("missing queue remove snapshot"))?;
                crate::logging::info(
                    "task_queue_message_removed",
                    serde_json::json!({
                        "task_id": task_id,
                        "queued_message_id": params.queued_message_id.as_str(),
                        "queue_revision": snapshot.message_queue.revision,
                        "queue_length": snapshot.message_queue.items.len(),
                    }),
                );
                self.project_task_snapshot(snapshot)
            })
            .map_err(|error| match error {
                error if error.message == "Task Message Queue changed" => {
                    conflict_error("Task Message Queue changed; refresh before removing")
                }
                error => error,
            })
    }

    /// Atomically removes one observed item and returns client-owned Composer resources.
    pub(super) fn queue_take_message(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskQueueTakeParams,
    ) -> Result<TaskQueueTakeResult, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let queued_message_id = params.queued_message_id.as_str().to_string();
        self.turn_acceptance.serialize(&task_id, || {
            let observed = self.read_interactive_task_for_client(&task_id, client_instance_id)?;
            if observed.message_queue.revision != params.queue_revision {
                return Err(conflict_error(
                    "Task Message Queue changed; refresh before editing in Composer",
                ));
            }
            let queued = observed
                .message_queue
                .items
                .iter()
                .find(|item| item.queued_message_id == queued_message_id)
                .cloned()
                .ok_or_else(|| conflict_error("Queued Message no longer exists"))?;
            validate_queued_attachment_paths(&observed, &queued.agent_attachments)?;
            let owner = crate::attachment_runtime::AttachmentOwner::new(
                client_instance_id,
                &params.task_id,
            );
            let (attachments, images) = self
                .attachments
                .restore_queued_for_composer(
                    &owner,
                    &queued.chat_attachments,
                    &queued.agent_attachments,
                )
                .map_err(super::send::support::protocol_error_from_attachment_runtime)?;
            let client = client_instance_id.clone();
            let mutation = self.mutations.commit_existing_task(
                &task_id,
                super::response_snapshot_options(),
                |ctx| {
                    crate::tasks::access::require_client_task_access(ctx.task(), &client)?;
                    if ctx.task().message_queue.revision != params.queue_revision {
                        return Err(RuntimeError::Conflict(
                            "Task Message Queue changed".to_string(),
                        ));
                    }
                    let index = ctx
                        .task()
                        .message_queue
                        .items
                        .iter()
                        .position(|item| item.queued_message_id == queued_message_id)
                        .ok_or_else(|| {
                            RuntimeError::Conflict("Queued Message no longer exists".to_string())
                        })?;
                    let task = ctx.task_mut();
                    task.message_queue.items.remove(index);
                    if task.message_queue.items.is_empty() {
                        task.message_queue.pause = None;
                    }
                    task.message_queue.revision = task.message_queue.revision.saturating_add(1);
                    task.updated_at = now_string();
                    Ok(TaskMutationResult::Changed)
                },
            );
            let mutation = match mutation {
                Ok(mutation) => mutation,
                Err(error) => {
                    let resources = attachments
                        .iter()
                        .map(|attachment| {
                            openaide_app_server_protocol::attachment::AttachmentResourceId::Handle {
                                id: attachment.handle_id.clone(),
                            }
                        })
                        .collect::<Vec<_>>();
                    self.attachments.release_resources(&owner, &resources);
                    return Err(protocol_error_from_runtime(error));
                }
            };
            let snapshot = match mutation.response_snapshot {
                Some(snapshot) => snapshot,
                None => {
                    let resources = attachments
                        .iter()
                        .map(|attachment| {
                            openaide_app_server_protocol::attachment::AttachmentResourceId::Handle {
                                id: attachment.handle_id.clone(),
                            }
                        })
                        .collect::<Vec<_>>();
                    self.attachments.release_resources(&owner, &resources);
                    return Err(internal_error("missing queue take snapshot"));
                }
            };
            let task = match self.project_task_snapshot(snapshot) {
                Ok(task) => task,
                Err(error) => {
                    let resources = attachments
                        .iter()
                        .map(|attachment| {
                            openaide_app_server_protocol::attachment::AttachmentResourceId::Handle {
                                id: attachment.handle_id.clone(),
                            }
                        })
                        .collect::<Vec<_>>();
                    self.attachments.release_resources(&owner, &resources);
                    return Err(error);
                }
            };
            crate::logging::info(
                "task_queue_message_taken",
                serde_json::json!({
                    "task_id": task_id,
                    "queued_message_id": queued_message_id,
                    "queue_revision": task.message_queue.revision,
                    "client_mutation_id": params.client_mutation_id.as_str(),
                }),
            );
            Ok(TaskQueueTakeResult {
                task,
                message: TakenQueuedMessage {
                    text: queued.text,
                    attachments,
                    images,
                },
            })
        })
    }

    pub(super) fn queue_move_message(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskQueueMoveParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let queued_message_id = params.queued_message_id.as_str().to_string();
        let target_index = params.target_index;
        let snapshot = self.queue_mutate(
            client_instance_id,
            params.task_id.as_str(),
            params.queue_revision,
            |items| {
                let from = items
                    .iter()
                    .position(|item| item.queued_message_id == params.queued_message_id.as_str())
                    .ok_or_else(|| {
                        RuntimeError::Conflict("Queued Message no longer exists".to_string())
                    })?;
                let item = items.remove(from);
                let target = usize::try_from(params.target_index)
                    .unwrap_or(usize::MAX)
                    .min(items.len());
                items.insert(target, item);
                Ok(())
            },
        )?;
        crate::logging::info(
            "task_queue_message_moved",
            serde_json::json!({
                "task_id": task_id,
                "queued_message_id": queued_message_id,
                "target_index": target_index,
                "queue_revision": snapshot.message_queue.revision,
                "client_mutation_id": params.client_mutation_id.as_str(),
            }),
        );
        Ok(snapshot)
    }

    fn queue_mutate(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &str,
        _queue_revision: u64,
        mutate: impl FnOnce(&mut Vec<QueuedMessageRecord>) -> Result<(), RuntimeError>,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.turn_acceptance.serialize(task_id, || {
            self.read_interactive_task_for_client(task_id, client_instance_id)?;
            let client = client_instance_id.clone();
            let result = self
                .mutations
                .commit_existing_task(task_id, super::response_snapshot_options(), |ctx| {
                    crate::tasks::access::require_client_task_access(ctx.task(), &client)?;
                    // Moving is non-destructive and addresses a durable row identity, so
                    // apply it to the latest queue instead of rejecting a drag that began
                    // before an Agent update advanced the queue revision.
                    mutate(&mut ctx.task_mut().message_queue.items)?;
                    let task = ctx.task_mut();
                    task.message_queue.revision = task.message_queue.revision.saturating_add(1);
                    task.updated_at = now_string();
                    Ok(TaskMutationResult::Changed)
                })
                .map_err(protocol_error_from_runtime)?;
            let snapshot = result
                .response_snapshot
                .ok_or_else(|| internal_error("missing queue mutation snapshot"))?;
            self.project_task_snapshot(snapshot)
        })
    }
}

pub(super) fn validate_queued_attachment_paths(
    task: &crate::storage::records::TaskRecord,
    attachments: &[crate::protocol::model::Attachment],
) -> Result<(), ProtocolError> {
    let allowed_roots = [
        Some(task.workspace_root.as_str()),
        task.project_root.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|root| std::fs::canonicalize(root).ok())
    .collect::<Vec<_>>();
    for attachment in attachments {
        let Some(path) = attachment.path.as_deref() else {
            continue;
        };
        let canonical = std::fs::canonicalize(path).map_err(|_| {
            validation_error(
                "message.attachments",
                "A queued attachment is no longer available",
            )
        })?;
        if !allowed_roots.iter().any(|root| canonical.starts_with(root)) {
            return Err(validation_error(
                "message.attachments",
                "A queued attachment is outside the Task workspace",
            ));
        }
    }
    Ok(())
}
