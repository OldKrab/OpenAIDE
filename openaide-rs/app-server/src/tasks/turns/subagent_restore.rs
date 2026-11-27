use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crate::protocol::errors::RuntimeError;
use crate::tasks::turn_events::TaskSessionEventSink;

use super::TurnRunner;

/// Work that must not delay the request that caused it.
pub(crate) type BackgroundJob = Box<dyn FnOnce() + Send>;

/// Starts a [`BackgroundJob`]; the default runs it on a named thread. Tests substitute a
/// runner they release by hand, so ordering against the request's reply is deterministic.
pub(crate) type BackgroundSpawner =
    Arc<dyn Fn(&'static str, BackgroundJob) -> Result<(), RuntimeError> + Send + Sync>;

pub(super) fn spawn_on_thread(name: &'static str, job: BackgroundJob) -> Result<(), RuntimeError> {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(job)
        .map(drop)
        .map_err(|error| RuntimeError::Internal(format!("background thread: {}", error.kind())))
}

impl TurnRunner {
    #[cfg(test)]
    pub(crate) fn with_background_spawner(mut self, background: BackgroundSpawner) -> Self {
        self.background = background;
        self
    }

    /// Restores the children of a replay on a background worker, through the Task's attached
    /// `sink` so the restore and the session's live Subagent events share one emission lock
    /// and text-chunk state. Each child reaches clients through the same Subagent catalog and
    /// history notifications as live traffic; a client subscribing meanwhile reads the
    /// current state in its baseline. A Subagent History never fails its Task ([ADR-0076]),
    /// so failures are logged and nothing is returned. A Task removed or rebound to another
    /// session before the worker starts is skipped.
    ///
    /// [ADR-0076]: ../../../../docs/adr/0076-subagent-histories-are-task-journal-chat-streams.md
    pub(crate) fn restore_replayed_subagents_in_background(
        &self,
        task_id: &str,
        sink: Arc<TaskSessionEventSink>,
        events: Vec<crate::agent::events::AgentReplayedSubagentEvent>,
    ) {
        if events.is_empty() {
            return;
        }
        let event_count = events.len();
        let queued_at = Instant::now();
        let mutations = self.mutations.clone();
        let owned_task_id = task_id.to_string();
        let session_id = sink.session_id().to_string();
        crate::logging::info(
            "task_subagent_replay_restore_queued",
            serde_json::json!({ "task_id": task_id, "event_count": event_count }),
        );
        let job: BackgroundJob = Box::new(move || {
            let queued_ms = queued_at.elapsed().as_millis() as u64;
            let skipped = match mutations.store().read_task(&owned_task_id) {
                Ok(task) if task.tombstoned => Some("task_removed"),
                Ok(task) if task.agent_session_id.as_deref() != Some(session_id.as_str()) => {
                    Some("session_changed")
                }
                Ok(_) => None,
                Err(RuntimeError::TaskNotFound(_)) => Some("task_removed"),
                Err(error) => {
                    crate::logging::warn(
                        "task_subagent_replay_restore_skipped",
                        serde_json::json!({
                            "task_id": owned_task_id,
                            "reason": "task_unreadable",
                            "error_kind": error.code(),
                            "queued_ms": queued_ms,
                        }),
                    );
                    return;
                }
            };
            if let Some(reason) = skipped {
                crate::logging::info(
                    "task_subagent_replay_restore_skipped",
                    serde_json::json!({
                        "task_id": owned_task_id,
                        "reason": reason,
                        "queued_ms": queued_ms,
                    }),
                );
                return;
            }
            crate::logging::info(
                "task_subagent_replay_restore_worker_started",
                serde_json::json!({ "task_id": owned_task_id, "queued_ms": queued_ms }),
            );
            // Logs its own start, counts, per-child failures and duration.
            sink.restore_replayed_subagents(events);
        });
        if let Err(error) = (self.background)("openaide-subagent-replay-restore", job) {
            crate::logging::warn(
                "task_subagent_replay_restore_dispatch_failed",
                serde_json::json!({
                    "task_id": task_id,
                    "event_count": event_count,
                    "error_kind": error.code(),
                }),
            );
        }
    }
}
