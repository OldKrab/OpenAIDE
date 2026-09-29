use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::task::{ComposerMessage, TaskQueueSendSelection, TaskSendParams};

use crate::protocol::model::TaskStatus;
use crate::storage::records::TaskRecord;
use crate::storage::records::{QueuedMessageRecord, TaskLifecycle, TaskMessageQueuePauseRecord};
use crate::tasks::mutation::TaskMutationResult;

use super::TaskProductApi;

/// Only process-local wake candidates live here. Durable queue state, checked under the
/// Send gate, owns eligibility. Restart requires explicit Send or Resume queue.
#[derive(Clone, Default)]
pub(super) struct ScheduledQueueCoordinator {
    tasks: Arc<Mutex<HashSet<String>>>,
    running: Arc<AtomicBool>,
}

enum QueueWake {
    Dormant,
    Waiting,
    Due(TaskSendParams),
}

pub(super) fn validate_schedule(value: Option<&str>) -> Result<(), ProtocolError> {
    if let Some(value) = value {
        let now = crate::time::now_string().parse::<u64>().unwrap_or_default();
        if !value
            .parse::<u64>()
            .is_ok_and(|time| time > now && time <= 8_640_000_000_000_000)
        {
            return Err(super::validation_error(
                "notBefore",
                "Choose a future date and time",
            ));
        }
    }
    Ok(())
}

pub(crate) fn queued_message_is_due(item: &QueuedMessageRecord, now: &str) -> bool {
    item.not_before.as_deref().is_none_or(|time| {
        time.parse::<u64>()
            .ok()
            .zip(now.parse::<u64>().ok())
            .is_some_and(|(time, now)| time <= now)
    })
}

fn queue_is_waiting(task: &TaskRecord, now: &str) -> bool {
    !task.tombstoned
        && task.lifecycle == TaskLifecycle::Open
        && task.message_queue.pause.is_none()
        && task.message_queue.items.first().is_some_and(|head| {
            task.status != TaskStatus::Inactive
                || task.active_turn_id.is_some()
                || !queued_message_is_due(head, now)
        })
}

impl TaskProductApi {
    pub(super) fn resume_scheduled_queue(
        &self,
        client: &openaide_app_server_protocol::ids::ClientInstanceId,
        params: openaide_app_server_protocol::task::TaskQueueResumeParams,
    ) -> Result<openaide_app_server_protocol::snapshot::TaskSnapshot, ProtocolError> {
        let started = Instant::now();
        let operation_id = uuid::Uuid::new_v4().to_string();
        crate::logging::info(
            "task_queue_resume_started",
            serde_json::json!({
                "task_id": params.task_id.as_str(), "operation_id": operation_id, "attempt": 1,
            }),
        );
        let result = self.turn_acceptance.serialize(params.task_id.as_str(), || {
            self.read_interactive_task_for_client(params.task_id.as_str(), client)?;
            let result = self
                .mutations
                .commit_existing_task(
                    params.task_id.as_str(),
                    super::response_snapshot_options(),
                    |ctx| {
                        if ctx.task().message_queue.revision != params.queue_revision {
                            return Err(crate::protocol::errors::RuntimeError::Conflict(
                                "Task Message Queue changed".into(),
                            ));
                        }
                        if ctx.task().status != TaskStatus::Inactive
                            || ctx.task().message_queue.items.is_empty()
                        {
                            return Err(crate::protocol::errors::RuntimeError::Conflict(
                                "Queue cannot be resumed".into(),
                            ));
                        }
                        if ctx.task().message_queue.pause.is_none() {
                            return Ok(TaskMutationResult::Unchanged);
                        }
                        let queue = &mut ctx.task_mut().message_queue;
                        queue.pause = None;
                        queue.revision = queue.revision.saturating_add(1);
                        Ok(TaskMutationResult::Changed)
                    },
                )
                .map_err(super::protocol_error_from_runtime)?;
            self.watch_scheduled_queue(params.task_id.as_str());
            self.project_task_snapshot(
                result
                    .response_snapshot
                    .ok_or_else(|| super::internal_error("missing queue resume snapshot"))?,
            )
        });
        crate::logging::info(
            "task_queue_resume_completed",
            serde_json::json!({
                "task_id": params.task_id.as_str(), "operation_id": operation_id, "attempt": 1,
                "outcome": if result.is_ok() { "resumed" } else { "error" },
                "error_class": result.as_ref().err().map(|error| format!("{:?}", error.code)),
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        result
    }

    pub(super) fn watch_scheduled_queue(&self, task_id: &str) {
        self.scheduled_queue
            .tasks
            .lock()
            .expect("scheduled queue poisoned")
            .insert(task_id.to_string());
    }

    pub(super) fn poll_scheduled_queue(&self) {
        if self
            .scheduled_queue
            .tasks
            .lock()
            .expect("scheduled queue poisoned")
            .is_empty()
            || self.scheduled_queue.running.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let api = self.clone();
        std::thread::spawn(move || {
            let tasks: Vec<_> = api
                .scheduled_queue
                .tasks
                .lock()
                .expect("scheduled queue poisoned")
                .iter()
                .cloned()
                .collect();
            for task_id in tasks {
                api.deliver_scheduled_queue_at(&task_id, &crate::time::now_string());
            }
            api.scheduled_queue.running.store(false, Ordering::Release);
        });
    }

    /// Eligibility and Send acceptance use one lock, so a simultaneous Send, Stop,
    /// reorder, removal or archive cannot turn a timer wake into accidental steering.
    pub(super) fn deliver_scheduled_queue_at(&self, task_id: &str, now: &str) {
        // A healthy waiting poll is not an interaction: taking the command gate
        // would supersede passive Native Session reconciliation every second.
        // This hint may defer one tick; admission below always rechecks under the gate.
        if self
            .store
            .task_journal()
            .inspect_task_record(task_id, |task| queue_is_waiting(task, now))
            .unwrap_or(false)
        {
            return;
        }
        // Record admission before taking the gate: one blocked Task also delays
        // later candidates in this worker. Future/active healthy polls stay quiet.
        let wake_started = Instant::now();
        let operation_id = uuid::Uuid::new_v4().to_string();
        crate::logging::info(
            "task_scheduled_wake_started",
            serde_json::json!({
                "task_id": task_id, "operation_id": operation_id, "attempt": 1,
            }),
        );
        let outcome = self.turn_acceptance.serialize(task_id, || {
            crate::logging::info("task_scheduled_wake_admitted", serde_json::json!({
                "task_id": task_id, "operation_id": operation_id,
                "wait_duration_ms": wake_started.elapsed().as_millis(), "attempt": 1,
            }));
            let wake = self
                .store
                .task_journal()
                .inspect_task_record(task_id, |task| {
                    if task.tombstoned
                        || task.lifecycle != TaskLifecycle::Open
                        || task.message_queue.pause.is_some()
                        || task.message_queue.items.is_empty()
                    {
                        return QueueWake::Dormant;
                    }
                    let head = &task.message_queue.items[0];
                    if queue_is_waiting(task, now) {
                        return QueueWake::Waiting;
                    }
                    QueueWake::Due(TaskSendParams {
                        task_id: task_id.to_string().into(),
                        message: ComposerMessage {
                            text: Some(head.text.clone()),
                            ..Default::default()
                        },
                        queue_selection: Some(TaskQueueSendSelection {
                            queued_message_id: head.queued_message_id.clone().into(),
                            queue_revision: task.message_queue.revision,
                        }),
                    })
                });
            let params = match wake {
                Ok(QueueWake::Due(params)) => params,
                Ok(QueueWake::Waiting) => return "waiting",
                Ok(QueueWake::Dormant) => {
                    self.unwatch_scheduled_queue(task_id);
                    return "dormant";
                }
                Err(error) => {
                    if !matches!(
                        error,
                        crate::protocol::errors::RuntimeError::TaskNotFound(_)
                    ) {
                        crate::logging::warn(
                            "task_scheduled_queue_read_failed",
                            serde_json::json!({
                                "task_id": task_id, "operation_id": operation_id, "error_class": error.reason(), "attempt": 1,
                            }),
                        );
                    }
                    self.unwatch_scheduled_queue(task_id);
                    return "read_failed";
                }
            };
            let queued_message_id = params
                .queue_selection
                .as_ref()
                .expect("timer Send always selects a queue item")
                .queued_message_id
                .clone();
            let started = Instant::now();
            crate::logging::info(
                "task_scheduled_delivery_started",
                serde_json::json!({
                    "task_id": task_id, "operation_id": operation_id, "queued_message_id": queued_message_id, "attempt": 1,
                }),
            );
            // Open Tasks have no client lease. The timer supplies no client resources;
            // Send resolves only the durable selected item's attachment ownership.
            let result = self.send_message_serialized(&"scheduled-delivery".into(), params);
            if let Err(error) = &result {
                // No automatic retry, including when acceptance succeeded but its response
                // failed. A consumed message is never reconstructed from this old snapshot.
                let mut still_pending = true;
                // Keep the structured attachment validation cause. No turn has
                // started, so "interrupted work" would hide the recovery action.
                let pause = if error.code == ProtocolErrorCode::ValidationFailed
                    && error.target.as_ref().and_then(|target| target.field.as_deref())
                        == Some("message.attachments")
                {
                    TaskMessageQueuePauseRecord::AttachmentUnavailable
                } else {
                    TaskMessageQueuePauseRecord::UnsuccessfulTurn
                };
                let pause_result =
                    self.mutations.commit_existing_task(
                        task_id,
                        super::response_snapshot_options(),
                        |ctx| {
                            still_pending =
                                ctx.task().message_queue.items.iter().any(|item| {
                                    item.queued_message_id == queued_message_id.as_str()
                                });
                            if !still_pending || ctx.task().active_turn_id.is_some() {
                                return Ok(TaskMutationResult::Unchanged);
                            }
                            let queue = &mut ctx.task_mut().message_queue;
                            queue.pause = Some(pause);
                            queue.revision = queue.revision.saturating_add(1);
                            Ok(TaskMutationResult::Changed)
                        },
                    );
                if let Err(error) = &pause_result {
                    crate::logging::warn(
                        "task_scheduled_queue_pause_failed",
                        serde_json::json!({
                            "task_id": task_id, "operation_id": operation_id, "queued_message_id": queued_message_id,
                            "error_class": error.reason(), "attempt": 1,
                        }),
                    );
                }
                // A failed response after acceptance must not orphan later scheduled items.
                // Their own identities remain eligible after normal turn completion.
                if still_pending || pause_result.is_err() {
                    self.unwatch_scheduled_queue(task_id);
                }
            }
            let outcome = if result.is_ok() { "accepted" } else { "error" };
            crate::logging::info(
                "task_scheduled_delivery_completed",
                serde_json::json!({
                    "task_id": task_id, "operation_id": operation_id, "queued_message_id": queued_message_id,
                    "outcome": outcome,
                    "error_class": result.err().map(|error| format!("{:?}", error.code)),
                    "duration_ms": started.elapsed().as_millis(), "attempt": 1,
                }),
            );
            outcome
        });
        crate::logging::info(
            "task_scheduled_wake_completed",
            serde_json::json!({
                "task_id": task_id, "operation_id": operation_id, "outcome": outcome,
                "duration_ms": wake_started.elapsed().as_millis(), "attempt": 1,
            }),
        );
    }

    fn unwatch_scheduled_queue(&self, task_id: &str) {
        self.scheduled_queue
            .tasks
            .lock()
            .expect("scheduled queue poisoned")
            .remove(task_id);
    }
}
