use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::protocol::errors::RuntimeError;

use super::model::{CommitBoundary, CommittedTaskBatch, TaskWrite};

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;

// Stream deltas vary widely in size, so admission bounds retained payload bytes
// rather than write count. One noisy Task can consume at most a quarter.
const GLOBAL_STREAM_BYTE_CAPACITY: usize = 8 * 1024 * 1024;
const PER_TASK_STREAM_BYTE_CAPACITY: usize = 2 * 1024 * 1024;
const CONTROL_CAPACITY: usize = 64;
const MAX_BATCH_AGE: Duration = Duration::from_millis(32);
const MAX_BATCH_BYTES: usize = 64 * 1024;
const MAX_BATCH_OPERATIONS: usize = 256;

pub(super) struct QueuedWrite {
    pub write: TaskWrite,
    pub reply: mpsc::Sender<Result<CommittedTaskBatch, RuntimeError>>,
    admitted_at: Instant,
}

#[derive(Default)]
struct PendingTask {
    writes: VecDeque<QueuedWrite>,
    queued_bytes: usize,
    queued_operations: usize,
    stream_bytes: usize,
    control_writes: usize,
}

#[derive(Default)]
struct SchedulerState {
    pending: HashMap<String, PendingTask>,
    ready: VecDeque<String>,
    global_stream_bytes: usize,
    global_control_writes: usize,
    peak_global_stream_bytes: usize,
    peak_task_stream_bytes: usize,
    shutdown_reply: Option<mpsc::Sender<()>>,
    closed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SchedulerMetrics {
    pub peak_global_stream_bytes: usize,
    pub peak_task_stream_bytes: usize,
}

/// Bounded fair admission shared by callers and the one physical writer.
/// Control capacity is independent, so a data flood cannot starve barriers.
pub(super) struct Scheduler {
    state: Mutex<SchedulerState>,
    changed: Condvar,
}

pub(super) enum NextWork {
    Batch {
        task_id: String,
        writes: Vec<QueuedWrite>,
    },
    Shutdown(mpsc::Sender<()>),
    Closed,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SchedulerState::default()),
            changed: Condvar::new(),
        }
    }

    pub fn admit(
        &self,
        write: TaskWrite,
        reply: mpsc::Sender<Result<CommittedTaskBatch, RuntimeError>>,
    ) -> Result<(), RuntimeError> {
        validate_write_size(&write)?;
        let mut state = self.state.lock().expect("Task scheduler poisoned");
        while !state.closed && state.shutdown_reply.is_none() && !has_capacity(&state, &write) {
            state = self.changed.wait(state).expect("Task scheduler poisoned");
        }
        if state.closed || state.shutdown_reply.is_some() {
            return Err(RuntimeError::Storage(
                "Task journal worker is unavailable".to_string(),
            ));
        }
        enqueue(&mut state, write, reply);
        self.changed.notify_one();
        Ok(())
    }

    /// Attempts admission without waiting. The caller retains ownership of a
    /// full-lane write so it can release unrelated locks before backpressure.
    pub fn try_admit(
        &self,
        write: TaskWrite,
        reply: mpsc::Sender<Result<CommittedTaskBatch, RuntimeError>>,
    ) -> Result<Option<TaskWrite>, RuntimeError> {
        validate_write_size(&write)?;
        let mut state = self.state.lock().expect("Task scheduler poisoned");
        if state.closed || state.shutdown_reply.is_some() {
            return Err(RuntimeError::Storage(
                "Task journal worker is unavailable".to_string(),
            ));
        }
        if !has_capacity(&state, &write) {
            return Ok(Some(write));
        }
        enqueue(&mut state, write, reply);
        self.changed.notify_one();
        Ok(None)
    }

    /// Waits only for an admission opportunity; callers must retry because a
    /// competing stream can consume capacity before they reacquire ownership.
    pub fn wait_for_capacity(&self, write: &TaskWrite) -> Result<(), RuntimeError> {
        let mut state = self.state.lock().expect("Task scheduler poisoned");
        while !state.closed && state.shutdown_reply.is_none() && !has_capacity(&state, write) {
            state = self.changed.wait(state).expect("Task scheduler poisoned");
        }
        if state.closed || state.shutdown_reply.is_some() {
            return Err(RuntimeError::Storage(
                "Task journal worker is unavailable".to_string(),
            ));
        }
        Ok(())
    }

    pub fn request_shutdown(&self, reply: mpsc::Sender<()>) -> Result<(), RuntimeError> {
        let mut state = self.state.lock().expect("Task scheduler poisoned");
        if state.closed || state.shutdown_reply.is_some() {
            return Err(RuntimeError::Storage(
                "Task journal worker is unavailable".to_string(),
            ));
        }
        state.shutdown_reply = Some(reply);
        self.changed.notify_all();
        Ok(())
    }

    pub fn close(&self) {
        let mut state = self.state.lock().expect("Task scheduler poisoned");
        state.closed = true;
        self.changed.notify_all();
    }

    /// Closes root-wide admission and resolves every queued receipt after the
    /// sole writer dies. This prevents callers from waiting forever on work
    /// that no thread can make durable.
    pub fn fail_all(&self, message: &str) {
        // Root-fatal cleanup must still run when a panic poisoned the scheduler
        // lock; the state is no longer used for normal scheduling afterward.
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        for queued in state
            .pending
            .values_mut()
            .flat_map(|task| task.writes.drain(..))
        {
            let _ = queued
                .reply
                .send(Err(RuntimeError::Storage(message.to_string())));
        }
        state.pending.clear();
        state.ready.clear();
        state.global_stream_bytes = 0;
        state.global_control_writes = 0;
        if let Some(reply) = state.shutdown_reply.take() {
            let _ = reply.send(());
        }
        self.changed.notify_all();
    }

    /// Returns high-water marks since this scheduler opened. These retained-byte
    /// measurements make overload benchmarks and production diagnostics honest.
    pub fn metrics(&self) -> SchedulerMetrics {
        let state = self.state.lock().expect("Task scheduler poisoned");
        SchedulerMetrics {
            peak_global_stream_bytes: state.peak_global_stream_bytes,
            peak_task_stream_bytes: state.peak_task_stream_bytes,
        }
    }

    pub fn next(&self) -> NextWork {
        let mut state = self.state.lock().expect("Task scheduler poisoned");
        loop {
            if let Some(task_id) = state.ready.front().cloned() {
                if should_wait_for_batch(&state, &task_id) {
                    let admitted_at = state.pending[&task_id]
                        .writes
                        .front()
                        .expect("ready Task has queued write")
                        .admitted_at;
                    let remaining = MAX_BATCH_AGE.saturating_sub(admitted_at.elapsed());
                    if !remaining.is_zero() {
                        let (next_state, timeout) = self
                            .changed
                            .wait_timeout(state, remaining)
                            .expect("Task scheduler poisoned");
                        state = next_state;
                        if !timeout.timed_out() {
                            continue;
                        }
                    }
                }
                let task_id = state.ready.pop_front().expect("front checked above");
                let writes = take_batch(&mut state, &task_id);
                self.changed.notify_all();
                return NextWork::Batch { task_id, writes };
            }
            if let Some(reply) = state.shutdown_reply.take() {
                state.closed = true;
                self.changed.notify_all();
                return NextWork::Shutdown(reply);
            }
            if state.closed {
                return NextWork::Closed;
            }
            state = self.changed.wait(state).expect("Task scheduler poisoned");
        }
    }
}

fn validate_write_size(write: &TaskWrite) -> Result<(), RuntimeError> {
    if write.boundary == CommitBoundary::Stream
        && write.estimated_bytes() > PER_TASK_STREAM_BYTE_CAPACITY
    {
        return Err(RuntimeError::Storage(
            "Task journal stream write exceeds admission capacity".to_string(),
        ));
    }
    Ok(())
}

fn has_capacity(state: &SchedulerState, write: &TaskWrite) -> bool {
    match write.boundary {
        CommitBoundary::Stream => {
            let write_bytes = write.estimated_bytes();
            let task_bytes = state
                .pending
                .get(&write.task_id)
                .map_or(0, |task| task.stream_bytes);
            write_bytes <= GLOBAL_STREAM_BYTE_CAPACITY.saturating_sub(state.global_stream_bytes)
                && write_bytes <= PER_TASK_STREAM_BYTE_CAPACITY.saturating_sub(task_bytes)
        }
        CommitBoundary::Barrier => state.global_control_writes < CONTROL_CAPACITY,
    }
}

fn enqueue(
    state: &mut SchedulerState,
    write: TaskWrite,
    reply: mpsc::Sender<Result<CommittedTaskBatch, RuntimeError>>,
) {
    let task_id = write.task_id.clone();
    let is_new = !state.pending.contains_key(&task_id);
    let boundary = write.boundary;
    let write_bytes = write.estimated_bytes();
    let entry = state.pending.entry(task_id.clone()).or_default();
    entry.queued_bytes = entry.queued_bytes.saturating_add(write_bytes);
    entry.queued_operations = entry
        .queued_operations
        .saturating_add(write.operations.len() + write.artifacts.len());
    match boundary {
        CommitBoundary::Stream => {
            entry.stream_bytes += write_bytes;
            state.global_stream_bytes += write_bytes;
            state.peak_global_stream_bytes = state
                .peak_global_stream_bytes
                .max(state.global_stream_bytes);
            state.peak_task_stream_bytes = state.peak_task_stream_bytes.max(entry.stream_bytes);
        }
        CommitBoundary::Barrier => {
            entry.control_writes += 1;
            state.global_control_writes += 1;
        }
    }
    entry.writes.push_back(QueuedWrite {
        write,
        reply,
        admitted_at: Instant::now(),
    });
    if is_new {
        state.ready.push_back(task_id);
    }
}

fn should_wait_for_batch(state: &SchedulerState, task_id: &str) -> bool {
    let task = &state.pending[task_id];
    state.ready.len() == 1
        && state.shutdown_reply.is_none()
        && task.control_writes == 0
        && task.queued_bytes < MAX_BATCH_BYTES
        && task.queued_operations < MAX_BATCH_OPERATIONS
}

fn take_batch(state: &mut SchedulerState, task_id: &str) -> Vec<QueuedWrite> {
    let task = state
        .pending
        .get_mut(task_id)
        .expect("ready Task has pending queue");
    let mut batch = Vec::new();
    let mut bytes = 0;
    let mut operations = 0;
    while let Some(front) = task.writes.front() {
        let next_bytes = front.write.estimated_bytes();
        let next_operations = front.write.operations.len() + front.write.artifacts.len();
        if !batch.is_empty()
            && (bytes + next_bytes > MAX_BATCH_BYTES
                || operations + next_operations > MAX_BATCH_OPERATIONS)
        {
            break;
        }
        let queued = task.writes.pop_front().expect("front checked above");
        task.queued_bytes = task.queued_bytes.saturating_sub(next_bytes);
        task.queued_operations = task.queued_operations.saturating_sub(next_operations);
        bytes += next_bytes;
        operations += next_operations;
        match queued.write.boundary {
            CommitBoundary::Stream => {
                task.stream_bytes -= next_bytes;
                state.global_stream_bytes -= next_bytes;
            }
            CommitBoundary::Barrier => {
                task.control_writes -= 1;
                state.global_control_writes -= 1;
            }
        }
        let boundary = queued.write.boundary;
        batch.push(queued);
        if boundary == CommitBoundary::Barrier {
            break;
        }
    }
    if task.writes.is_empty() {
        state.pending.remove(task_id);
    } else {
        state.ready.push_back(task_id.to_string());
    }
    batch
}
