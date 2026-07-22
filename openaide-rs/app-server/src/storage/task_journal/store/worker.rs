use std::sync::{mpsc, Arc, Mutex};

use crate::storage::task_journal::model::{CommittedTaskBatch, TaskStorageFailure};
use crate::storage::task_journal::scheduler::{NextWork, Scheduler};

use super::{commit_batch, current_commit, resolve_batch, CommitContext};

pub(super) fn run(
    context: CommitContext,
    scheduler: Arc<Scheduler>,
    commit_subscribers: Arc<Mutex<Vec<mpsc::Sender<CommittedTaskBatch>>>>,
    failure_subscribers: Arc<Mutex<Vec<mpsc::Sender<TaskStorageFailure>>>>,
) {
    loop {
        match scheduler.next() {
            NextWork::Batch { task_id, writes } => {
                context.faults.panic_if_armed();
                let result = commit_batch(&context, &task_id, &writes);
                if let Ok(Some(committed)) = &result {
                    broadcast(&commit_subscribers, committed.clone());
                } else if result.is_err() {
                    broadcast(
                        &failure_subscribers,
                        TaskStorageFailure {
                            task_id: task_id.clone(),
                        },
                    );
                }
                let receipt_result = match result {
                    Ok(Some(committed)) => Ok(committed),
                    Ok(None) => current_commit(&context.projections, &task_id),
                    Err(error) => Err(error),
                };
                resolve_batch(writes, receipt_result);
            }
            NextWork::Shutdown(reply) => {
                let _ = reply.send(());
                return;
            }
            NextWork::Closed => return,
        }
    }
}

fn broadcast<T: Clone>(subscribers: &Mutex<Vec<mpsc::Sender<T>>>, event: T) {
    subscribers
        .lock()
        .expect("Task journal subscribers poisoned")
        .retain(|subscriber| subscriber.send(event.clone()).is_ok());
}
