//! Single consumer of committed Task updates for the Local HTTP App Server.
//!
//! Every publication takes the protocol lock that client requests also need. A navigation
//! update is only a wake: the commit already marked its Project dirty, and releasing the lock
//! flushes every mark into frames. So when the queue backs up one wake covers them all. Other
//! updates carry captured values or ordered deltas and are always published, in order. The
//! publisher coalesces only what is already queued and never waits for more, so an idle queue
//! publishes immediately and a saturated one sheds redundant wakes.

use std::time::{Duration, Instant};

use serde_json::json;

use crate::client_lifecycle::AppServerTime;
use crate::task_events::{TaskUpdate, TaskUpdateKind, TaskUpdateReceiver};

use super::SharedRpcGateway;

#[cfg(test)]
#[path = "task_update_publisher_tests.rs"]
mod tests;

/// Protocol lock timing of one committed Task update publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskUpdatePublication {
    /// Time spent queued behind other holders of the protocol lock.
    pub lock_wait: Duration,
    /// Time the publication held the lock while queueing events.
    pub lock_held: Duration,
}

/// Bounds how many queued updates one batch takes.
const MAX_BATCH: usize = 1024;

/// A batch that took this long explains request latency even without coalescing.
const SLOW_BATCH: Duration = Duration::from_millis(250);

/// Publishes Task updates until every notifier is dropped.
pub fn run_task_update_publisher(updates: TaskUpdateReceiver, gateway: SharedRpcGateway) {
    publish_task_updates(
        updates,
        SLOW_BATCH,
        |update| gateway.publish_committed_task_update(update, AppServerTime::now()),
        crate::logging::info,
    );
}

/// Publishing and logging are injected so tests observe batches without the global logger.
pub(super) fn publish_task_updates(
    updates: TaskUpdateReceiver,
    slow_batch: Duration,
    mut publish: impl FnMut(&TaskUpdate) -> TaskUpdatePublication,
    log: impl Fn(&str, serde_json::Value),
) {
    log(
        "task_update_publisher_started",
        json!({ "operation": "task_update_publication" }),
    );
    let started_at = Instant::now();
    let (mut received_total, mut published_total) = (0usize, 0usize);
    while let Ok(first) = updates.recv() {
        let batch_started_at = Instant::now();
        let mut batch = vec![first];
        batch.extend(updates.try_iter().take(MAX_BATCH - 1));
        let received = batch.len();
        let batch = coalesce(batch);
        let mut stats = BatchStats::default();
        for update in &batch {
            stats.record(update.kind.diagnostic_name(), publish(update));
        }
        received_total += received;
        published_total += batch.len();
        let coalesced = received - batch.len();
        let duration = batch_started_at.elapsed();
        if coalesced > 0 || duration >= slow_batch {
            // Healthy single-update batches stay quiet; backlog and slow publication are
            // the evidence for request latency on the shared protocol lock.
            log(
                "task_update_batch_published",
                json!({
                    "operation": "task_update_publication",
                    "outcome": "published",
                    "count": received,
                    "published_count": batch.len(),
                    "coalesced_count": coalesced,
                    "wait_ms": stats.lock_wait.as_millis(),
                    "max_hold_ms": stats.max_held.as_millis(),
                    "update_kind": stats.max_held_kind,
                    "duration_ms": duration.as_millis(),
                }),
            );
        }
    }
    log(
        "task_update_publisher_stopped",
        json!({
            "operation": "task_update_publication",
            "outcome": "closed",
            "count": received_total,
            "published_count": published_total,
            "coalesced_count": received_total - published_total,
            "duration_ms": started_at.elapsed().as_millis(),
        }),
    );
}

#[derive(Default)]
struct BatchStats {
    lock_wait: Duration,
    max_held: Duration,
    max_held_kind: Option<&'static str>,
}

impl BatchStats {
    fn record(&mut self, kind: &'static str, publication: TaskUpdatePublication) {
        self.lock_wait += publication.lock_wait;
        if self.max_held_kind.is_none() || publication.lock_held > self.max_held {
            self.max_held = publication.lock_held;
            self.max_held_kind = Some(kind);
        }
    }
}

/// Keeps the first navigation wake of a batch: every mark its siblings stand for was set before
/// the batch was read, so the flush the first one triggers covers them.
fn coalesce(batch: Vec<TaskUpdate>) -> Vec<TaskUpdate> {
    let mut woke = false;
    batch
        .into_iter()
        .filter(|update| match update.kind {
            TaskUpdateKind::NavigationDirty => !std::mem::replace(&mut woke, true),
            _ => true,
        })
        .collect()
}
