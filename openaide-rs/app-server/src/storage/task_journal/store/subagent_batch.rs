//! Commits a batch of Subagent writes from the Task's queue (ADR-0076). Unlike a
//! Task batch, a failure here never freezes the Task: each write gets its own
//! outcome, and a failed history becomes unavailable on its own.

use std::sync::{mpsc, Mutex};

use crate::storage::task_journal::model::{CommitBoundary, CommittedTaskBatch};
use crate::storage::task_journal::scheduler::QueuedWrite;

use super::{CommitContext, RecoveredTask};

pub(super) fn commit(
    context: &CommitContext,
    task_id: &str,
    writes: Vec<QueuedWrite>,
    commit_subscribers: &Mutex<Vec<mpsc::Sender<CommittedTaskBatch>>>,
) {
    let subagent_writes = writes
        .iter()
        .filter_map(|queued| queued.write.subagent.as_deref())
        .collect::<Vec<_>>();
    let (committed, outcomes) =
        context
            .subagents
            .commit_batch(&context.tasks_root, task_id, &subagent_writes);
    let reply = unchanged_task_commit(context, task_id);
    if let Some(subagent) = committed {
        super::worker::broadcast(
            commit_subscribers,
            CommittedTaskBatch {
                subagent: Some(subagent),
                ..reply.clone()
            },
        );
    }
    for (queued, outcome) in writes.into_iter().zip(outcomes) {
        if let Err(error) = &outcome {
            // A streamed write's caller dropped its receipt, so this is the
            // failure's only record.
            if matches!(queued.write.boundary, CommitBoundary::Stream) {
                let write = queued.write.subagent.as_deref().expect("Subagent batch");
                crate::logging::warn(
                    "subagent_stream_write_failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "operation": write.operation_kind(),
                        "error_kind": error.reason(),
                    }),
                );
            }
        }
        // Callers learn only the outcome; the published batch carries the changes.
        let _ = queued.reply.send(outcome.map(|()| reply.clone()));
    }
}

/// The Task's current commit facts, which a Subagent batch leaves unchanged. A
/// Task whose own state is not loaded reports zero, which no Subagent caller reads.
fn unchanged_task_commit(context: &CommitContext, task_id: &str) -> CommittedTaskBatch {
    let projections = context
        .projections
        .read()
        .expect("Task journal projections poisoned");
    let (journal_sequence, task_revision) = match projections.get(task_id) {
        Some(RecoveredTask::Available {
            journal_sequence,
            projection,
        }) => (*journal_sequence, projection.task.revision),
        _ => (0, 0),
    };
    CommittedTaskBatch {
        task_id: task_id.to_string(),
        subagent: None,
        journal_sequence,
        task_revision,
        task_snapshot_changed: false,
        chat_text_appends: Vec::new(),
        streamed_text_write_count: 0,
        replaced_artifact_ids: Vec::new(),
        artifact_changes: Vec::new(),
    }
}
