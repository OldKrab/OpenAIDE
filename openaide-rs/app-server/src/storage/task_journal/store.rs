use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;

use super::artifact;
use super::frame;
use super::model::{
    ArtifactOperation, CommittedTaskBatch, JournalFrame, TaskOperation, TaskProjection, TaskWrite,
    ToolArtifactProjection,
};
use super::projection::{apply_operations, replay_tasks, validate_operations};
use super::scheduler::{NextWork, QueuedWrite, Scheduler};

const TASK_STORE_DIR: &str = "task-store-v1";
const TASKS_DIR: &str = "tasks";
pub(super) const JOURNAL_FILE: &str = "task.journal";

/// Handle for one admitted write. Waiting establishes durability; dropping the
/// handle leaves the admitted write owned by the storage worker.
pub struct CommitReceipt {
    receiver: Receiver<Result<CommittedTaskBatch, RuntimeError>>,
}

impl CommitReceipt {
    pub fn wait(self) -> Result<CommittedTaskBatch, RuntimeError> {
        self.receiver.recv().map_err(|_| {
            RuntimeError::Storage("Task journal worker stopped before commit".to_string())
        })?
    }
}

/// Deep Task persistence module. One worker owns ordering and physical writes;
/// callers observe recovered projections and durable commit facts only.
#[derive(Clone)]
pub struct TaskJournalStore {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    scheduler: Arc<Scheduler>,
    projections: Arc<RwLock<HashMap<String, RecoveredTask>>>,
    tasks_root: PathBuf,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl Drop for StoreInner {
    fn drop(&mut self) {
        self.scheduler.close();
        if let Some(worker) = self
            .worker
            .get_mut()
            .expect("Task journal worker handle poisoned")
            .take()
        {
            let _ = worker.join();
        }
    }
}

pub(super) enum RecoveredTask {
    Available {
        projection: Box<TaskProjection>,
        journal_sequence: u64,
    },
    Unavailable {
        error: String,
    },
}

impl TaskJournalStore {
    pub fn open(state_root: PathBuf) -> Result<(Self, Receiver<CommittedTaskBatch>), RuntimeError> {
        let tasks_root = state_root.join(TASK_STORE_DIR).join(TASKS_DIR);
        fs::create_dir_all(&tasks_root)?;
        let recovered = replay_tasks(&tasks_root)?;
        artifact::reconcile(&tasks_root, &recovered)?;
        let projections = Arc::new(RwLock::new(recovered));
        let scheduler = Arc::new(Scheduler::new());
        let (commit_sender, commits) = mpsc::channel();
        let worker_projections = projections.clone();
        let worker_tasks_root = tasks_root.clone();
        let worker_scheduler = scheduler.clone();
        let worker = thread::Builder::new()
            .name("openaide-task-journal".to_string())
            .spawn(move || {
                run_worker(
                    worker_tasks_root,
                    worker_projections,
                    worker_scheduler,
                    commit_sender,
                )
            })
            .map_err(RuntimeError::from)?;

        Ok((
            Self {
                inner: Arc::new(StoreInner {
                    scheduler,
                    projections,
                    tasks_root,
                    worker: Mutex::new(Some(worker)),
                }),
            },
            commits,
        ))
    }

    pub fn submit(&self, write: TaskWrite) -> Result<CommitReceipt, RuntimeError> {
        validate_task_id(&write.task_id)?;
        let (reply, receiver) = mpsc::channel();
        self.inner.scheduler.admit(write, reply)?;
        Ok(CommitReceipt { receiver })
    }

    pub fn load(&self, task_id: &str) -> Result<TaskProjection, RuntimeError> {
        validate_task_id(task_id)?;
        self.inner
            .projections
            .read()
            .expect("Task journal projections poisoned")
            .get(task_id)
            .map(|task| match task {
                RecoveredTask::Available { projection, .. } => Ok(projection.as_ref().clone()),
                RecoveredTask::Unavailable { error } => Err(RuntimeError::Storage(error.clone())),
            })
            .transpose()?
            .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))
    }

    /// Loads Tool detail only when requested, bounded by the artifact head
    /// durably referenced from the Task journal.
    pub fn load_tool_artifact(
        &self,
        task_id: &str,
        artifact_id: &str,
    ) -> Result<ToolArtifactProjection, RuntimeError> {
        validate_task_id(task_id)?;
        artifact::validate_artifact_id(artifact_id)?;
        let committed_head = {
            let state = self
                .inner
                .projections
                .read()
                .expect("Task journal projections poisoned");
            let projection = match state.get(task_id) {
                Some(RecoveredTask::Available { projection, .. }) => projection,
                Some(RecoveredTask::Unavailable { error }) => {
                    return Err(RuntimeError::Storage(error.clone()))
                }
                None => return Err(RuntimeError::TaskNotFound(task_id.to_string())),
            };
            projection
                .artifact_heads
                .get(artifact_id)
                .copied()
                .ok_or_else(|| {
                    RuntimeError::Storage(format!("Tool artifact is not committed: {artifact_id}"))
                })?
        };
        artifact::load(&self.inner.tasks_root, task_id, artifact_id, committed_head)
    }

    /// Flushes all preceding writes and joins the single storage worker.
    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        let mut worker = self
            .inner
            .worker
            .lock()
            .expect("Task journal worker handle poisoned");
        let Some(handle) = worker.take() else {
            return Ok(());
        };
        let (reply, receiver) = mpsc::channel();
        self.inner.scheduler.request_shutdown(reply)?;
        receiver.recv().map_err(|_| {
            RuntimeError::Storage("Task journal worker stopped during shutdown".to_string())
        })?;
        handle.join().map_err(|_| {
            RuntimeError::Storage("Task journal worker panicked during shutdown".to_string())
        })?;
        Ok(())
    }
}

fn run_worker(
    tasks_root: PathBuf,
    projections: Arc<RwLock<HashMap<String, RecoveredTask>>>,
    scheduler: Arc<Scheduler>,
    commits: mpsc::Sender<CommittedTaskBatch>,
) {
    loop {
        match scheduler.next() {
            NextWork::Batch { task_id, writes } => {
                let result = commit_batch(&tasks_root, &projections, &task_id, &writes);
                if let Ok(Some(committed)) = &result {
                    let _ = commits.send(committed.clone());
                }
                let receipt_result = match result {
                    Ok(Some(committed)) => Ok(committed),
                    Ok(None) => current_commit(&projections, &task_id),
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

fn commit_batch(
    tasks_root: &Path,
    projections: &RwLock<HashMap<String, RecoveredTask>>,
    task_id: &str,
    batch: &[QueuedWrite],
) -> Result<Option<CommittedTaskBatch>, RuntimeError> {
    let mut reduced = reduce_batch(batch);
    let mut state = projections
        .write()
        .expect("Task journal projections poisoned");
    if reduced.task_operations.is_empty() && reduced.artifacts.is_empty() {
        require_available(&state, task_id)?;
        return Ok(None);
    }
    let task_snapshot_changed = reduced.task_operations.iter().any(|operation| {
        matches!(
            operation,
            TaskOperation::Create { .. }
                | TaskOperation::ReplaceTask { .. }
                | TaskOperation::AppendText { .. }
        )
    });
    let mut artifact_changes = Vec::new();
    if !reduced.artifacts.is_empty() {
        let projection = match state.get(task_id) {
            Some(RecoveredTask::Available { projection, .. }) => projection,
            Some(RecoveredTask::Unavailable { error }) => {
                return Err(RuntimeError::Storage(error.clone()))
            }
            None => return Err(RuntimeError::TaskNotFound(task_id.to_string())),
        };
        for (artifact_id, operations) in reduced.artifacts {
            let committed_head = projection
                .artifact_heads
                .get(&artifact_id)
                .copied()
                .unwrap_or_default();
            let change = match artifact::prepare(
                tasks_root,
                task_id,
                &artifact_id,
                committed_head,
                operations,
            ) {
                Ok(change) => change,
                Err(error) => return Err(freeze_task(&mut state, task_id, error)),
            };
            reduced.task_operations.push(TaskOperation::CommitArtifact {
                artifact_id,
                artifact_sequence: change.artifact_sequence,
            });
            artifact_changes.push(change);
        }
    }
    validate_operations(state.get(task_id), task_id, &reduced.task_operations)?;
    let sequence = match state.get(task_id) {
        Some(RecoveredTask::Available {
            journal_sequence, ..
        }) => journal_sequence
            .checked_add(1)
            .ok_or_else(|| RuntimeError::Storage("Task journal sequence overflow".to_string()))?,
        Some(RecoveredTask::Unavailable { error }) => {
            return Err(RuntimeError::Storage(error.clone()))
        }
        None => 1,
    };
    let frame = JournalFrame {
        format_version: 1,
        sequence,
        operations: reduced.task_operations,
    };
    let journal = journal_path(tasks_root, task_id)?;
    let persisted = if sequence == 1 {
        frame::create(&journal, &frame)
    } else {
        frame::append(&journal, &frame)
    };
    if let Err(error) = persisted {
        return Err(freeze_task(&mut state, task_id, error));
    }
    apply_operations(&mut state, task_id, frame.operations, sequence)?;
    Ok(Some(CommittedTaskBatch {
        task_id: task_id.to_string(),
        journal_sequence: sequence,
        task_snapshot_changed,
        artifact_changes,
    }))
}

fn freeze_task(
    state: &mut HashMap<String, RecoveredTask>,
    task_id: &str,
    error: RuntimeError,
) -> RuntimeError {
    let message = format!("Task storage is frozen after a durability failure: {error}");
    crate::logging::warn(
        "task_journal_frozen",
        serde_json::json!({ "task_id": task_id, "error": error.to_string() }),
    );
    state.insert(
        task_id.to_string(),
        RecoveredTask::Unavailable {
            error: message.clone(),
        },
    );
    RuntimeError::Storage(message)
}

struct ReducedBatch {
    task_operations: Vec<TaskOperation>,
    artifacts: BTreeMap<String, Vec<ArtifactOperation>>,
}

fn reduce_batch(batch: &[QueuedWrite]) -> ReducedBatch {
    let mut task_operations = Vec::new();
    let mut artifacts = BTreeMap::<String, Vec<ArtifactOperation>>::new();
    for queued in batch {
        for operation in &queued.write.operations {
            match operation {
                TaskOperation::AppendText {
                    identity,
                    text,
                    local_history_updated_at,
                } => match task_operations.last_mut() {
                    Some(TaskOperation::AppendText {
                        identity: existing_identity,
                        text: existing_text,
                        local_history_updated_at: existing_updated_at,
                    }) if existing_identity == identity => {
                        existing_text.push_str(text);
                        existing_updated_at.clone_from(local_history_updated_at);
                    }
                    _ => task_operations.push(TaskOperation::AppendText {
                        identity: identity.clone(),
                        text: text.clone(),
                        local_history_updated_at: local_history_updated_at.clone(),
                    }),
                },
                TaskOperation::Create { projection } => {
                    task_operations.push(TaskOperation::Create {
                        projection: projection.clone(),
                    })
                }
                TaskOperation::ReplaceTask { task } => {
                    task_operations.push(TaskOperation::ReplaceTask { task: task.clone() })
                }
                TaskOperation::CommitArtifact { .. } => {
                    unreachable!("artifact commit references are worker-owned")
                }
            }
        }
        for write in &queued.write.artifacts {
            match &write.operation {
                ArtifactOperation::AppendTerminal { terminal_id, data } => {
                    if data.is_empty() {
                        continue;
                    }
                    let operations = artifacts.entry(write.artifact_id.clone()).or_default();
                    match operations.last_mut() {
                        Some(ArtifactOperation::AppendTerminal {
                            terminal_id: existing_terminal_id,
                            data: existing_data,
                        }) if existing_terminal_id == terminal_id => existing_data.push_str(data),
                        _ => operations.push(write.operation.clone()),
                    }
                }
            }
        }
    }
    ReducedBatch {
        task_operations,
        artifacts,
    }
}

fn resolve_batch(batch: Vec<QueuedWrite>, result: Result<CommittedTaskBatch, RuntimeError>) {
    for reply in batch.into_iter().map(|queued| queued.reply) {
        let response = match &result {
            Ok(committed) => Ok(committed.clone()),
            Err(error) => Err(RuntimeError::Storage(error.to_string())),
        };
        let _ = reply.send(response);
    }
}

fn current_commit(
    projections: &RwLock<HashMap<String, RecoveredTask>>,
    task_id: &str,
) -> Result<CommittedTaskBatch, RuntimeError> {
    let state = projections
        .read()
        .expect("Task journal projections poisoned");
    let sequence = match state.get(task_id) {
        Some(RecoveredTask::Available {
            journal_sequence, ..
        }) => *journal_sequence,
        Some(RecoveredTask::Unavailable { error }) => {
            return Err(RuntimeError::Storage(error.clone()))
        }
        None => return Err(RuntimeError::TaskNotFound(task_id.to_string())),
    };
    Ok(CommittedTaskBatch {
        task_id: task_id.to_string(),
        journal_sequence: sequence,
        task_snapshot_changed: false,
        artifact_changes: Vec::new(),
    })
}

fn require_available(
    state: &HashMap<String, RecoveredTask>,
    task_id: &str,
) -> Result<(), RuntimeError> {
    match state.get(task_id) {
        Some(RecoveredTask::Available { .. }) => Ok(()),
        Some(RecoveredTask::Unavailable { error }) => Err(RuntimeError::Storage(error.clone())),
        None => Err(RuntimeError::TaskNotFound(task_id.to_string())),
    }
}

fn journal_path(tasks_root: &Path, task_id: &str) -> Result<PathBuf, RuntimeError> {
    validate_task_id(task_id)?;
    Ok(tasks_root.join(task_id).join(JOURNAL_FILE))
}
