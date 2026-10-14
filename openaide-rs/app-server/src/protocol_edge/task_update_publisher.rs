//! Single consumer of committed Task updates for the Local HTTP App Server.
//!
//! Every publication takes the protocol lock that client requests also need. Navigation
//! updates are invalidations: their publication re-reads authoritative state, so when the
//! queue backs up only the latest invalidation per target needs to run. Other updates carry
//! captured values or ordered deltas and are always published, in order. The publisher
//! coalesces only what is already queued and never waits for more, so an idle queue
//! publishes immediately and a saturated one sheds redundant navigation projections.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use openaide_app_server_protocol::ids::ProjectId;
use serde_json::json;

use crate::client_lifecycle::AppServerTime;
use crate::task_events::{
    CommittedNavigationChange, TaskUpdate, TaskUpdateKind, TaskUpdateReceiver,
};

use super::{SharedRpcGateway, TaskUpdatePublication};

#[cfg(test)]
#[path = "task_update_publisher_tests.rs"]
mod tests;

/// Bounds how far a queued navigation invalidation can move behind later updates.
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

/// Keeps the last navigation invalidation per target at its own queue position, so the
/// projection it triggers is read after every update that preceded it in the batch.
fn coalesce(batch: Vec<TaskUpdate>) -> Vec<TaskUpdate> {
    let mut later_projects: HashSet<ProjectId> = HashSet::new();
    let mut later_refresh_state = false;
    let mut kept: Vec<TaskUpdate> = batch
        .into_iter()
        .rev()
        .filter(|update| match &update.kind {
            TaskUpdateKind::NavigationProjectEntriesChanged { project_id } => {
                later_projects.insert(project_id.clone())
            }
            TaskUpdateKind::NavigationRefreshStateChanged { .. } => {
                !std::mem::replace(&mut later_refresh_state, true)
            }
            TaskUpdateKind::Changed(change) => {
                // A committed change that re-projects its Project also satisfies earlier
                // invalidations of it; the change itself carries deltas and always stays.
                if let Some(CommittedNavigationChange::ProjectEntriesChanged { project_id }) =
                    &change.navigation
                {
                    later_projects.insert(project_id.clone());
                }
                true
            }
            _ => true,
        })
        .collect();
    kept.reverse();
    kept
}
