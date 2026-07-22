use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;
use crate::storage::records::TaskRecord;

use super::artifact;
use super::frame;
use super::model::{
    ArtifactOperation, CommittedTaskBatch, CompactionMode, JournalFrame, TaskJournalQueueMetrics,
    TaskOperation, TaskProjection, TaskStorageFailure, TaskStorageFatalFailure, TaskWrite,
    ToolArtifactProjection,
};
use super::projection::{apply_operations, validate_operations};
use super::scheduler::{QueuedWrite, Scheduler};

mod admission;
mod compaction;
pub(super) mod failure;
mod recovery;
mod worker;
pub(crate) use admission::TrySubmit;

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
    catalog: Arc<RwLock<HashMap<String, TaskRecord>>>,
    projections: Arc<RwLock<HashMap<String, RecoveredTask>>>,
    projection_load_lock: Arc<Mutex<()>>,
    artifact_heads: Arc<Mutex<artifact::ReconciledArtifactHeads>>,
    tasks_root: PathBuf,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    failure_subscribers: Arc<Mutex<Vec<mpsc::Sender<TaskStorageFailure>>>>,
    fatal_events: Mutex<Option<Receiver<TaskStorageFatalFailure>>>,
    faults: Arc<frame::FaultInjector>,
}

#[derive(Clone)]
struct CommitContext {
    tasks_root: PathBuf,
    catalog: Arc<RwLock<HashMap<String, TaskRecord>>>,
    projections: Arc<RwLock<HashMap<String, RecoveredTask>>>,
    projection_load_lock: Arc<Mutex<()>>,
    artifact_heads: Arc<Mutex<artifact::ReconciledArtifactHeads>>,
    faults: Arc<frame::FaultInjector>,
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

#[derive(Clone)]
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
        Self::open_with_faults(state_root, Arc::new(frame::FaultInjector::disabled()))
    }

    fn open_with_faults(
        state_root: PathBuf,
        faults: Arc<frame::FaultInjector>,
    ) -> Result<(Self, Receiver<CommittedTaskBatch>), RuntimeError> {
        let store_root = state_root.join(TASK_STORE_DIR);
        frame::create_directory_durably(&store_root, frame::JournalKind::Root, faults.as_ref())?;
        let tasks_root = store_root.join(TASKS_DIR);
        frame::create_directory_durably(&tasks_root, frame::JournalKind::Root, faults.as_ref())?;
        let (catalog_records, initially_recovered) = recovery::open_catalog(&tasks_root)?;
        failure::ensure_recovered_statuses(&tasks_root, &initially_recovered)?;
        let catalog = Arc::new(RwLock::new(catalog_records));
        let projections = Arc::new(RwLock::new(initially_recovered));
        let projection_load_lock = Arc::new(Mutex::new(()));
        let artifact_heads = Arc::new(Mutex::new(HashMap::new()));
        let scheduler = Arc::new(Scheduler::new());
        let commit_subscribers = Arc::new(Mutex::new(Vec::new()));
        let failure_subscribers = Arc::new(Mutex::new(Vec::new()));
        let (commit_sender, commits) = mpsc::channel();
        let (fatal_sender, fatal_events) = mpsc::channel();
        commit_subscribers
            .lock()
            .expect("Task commit subscribers poisoned")
            .push(commit_sender);
        let commit_context = CommitContext {
            tasks_root: tasks_root.clone(),
            catalog: catalog.clone(),
            projections: projections.clone(),
            projection_load_lock: projection_load_lock.clone(),
            artifact_heads: artifact_heads.clone(),
            faults: faults.clone(),
        };
        let worker_scheduler = scheduler.clone();
        let worker_commit_subscribers = commit_subscribers.clone();
        let worker_failure_subscribers = failure_subscribers.clone();
        let worker = thread::Builder::new()
            .name("openaide-task-journal".to_string())
            .spawn(move || {
                crate::logging::info("task_journal_worker_started", serde_json::json!({}));
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    worker::run(
                        commit_context,
                        worker_scheduler.clone(),
                        worker_commit_subscribers,
                        worker_failure_subscribers,
                    )
                }));
                if result.is_err() {
                    let message = "Task journal worker stopped after a root-wide failure";
                    worker_scheduler.fail_all(message);
                    crate::logging::error(
                        "task_journal_worker_fatal",
                        serde_json::json!({ "reason": "worker_panicked" }),
                    );
                    let _ = fatal_sender.send(TaskStorageFatalFailure {
                        reason: "worker_panicked",
                    });
                } else {
                    crate::logging::info("task_journal_worker_stopped", serde_json::json!({}));
                }
            })
            .map_err(RuntimeError::from)?;

        Ok((
            Self {
                inner: Arc::new(StoreInner {
                    scheduler,
                    catalog,
                    projections,
                    projection_load_lock,
                    artifact_heads,
                    tasks_root,
                    worker: Mutex::new(Some(worker)),
                    failure_subscribers,
                    fatal_events: Mutex::new(Some(fatal_events)),
                    faults,
                }),
            },
            commits,
        ))
    }

    /// Subscribes to path-free storage failures used to stop unsafe live work.
    pub fn subscribe_failures(&self) -> Receiver<TaskStorageFailure> {
        let (sender, receiver) = mpsc::channel();
        self.inner
            .failure_subscribers
            .lock()
            .expect("Task failure subscribers poisoned")
            .push(sender);
        receiver
    }

    /// Transfers the sole root-fatal stream to the App Server process
    /// supervisor. Multiple consumers could race and leave the process alive.
    pub(crate) fn take_fatal_events(&self) -> Receiver<TaskStorageFatalFailure> {
        self.inner
            .fatal_events
            .lock()
            .expect("Task journal fatal receiver poisoned")
            .take()
            .expect("Task journal fatal stream already has an owner")
    }

    pub fn submit(&self, write: TaskWrite) -> Result<CommitReceipt, RuntimeError> {
        validate_task_id(&write.task_id)?;
        let (reply, receiver) = mpsc::channel();
        self.inner.scheduler.admit(write, reply)?;
        Ok(CommitReceipt { receiver })
    }

    /// Reports observed retained stream payload, rather than queue length,
    /// because one ACP update can be much larger than another.
    pub fn queue_metrics(&self) -> TaskJournalQueueMetrics {
        let metrics = self.inner.scheduler.metrics();
        TaskJournalQueueMetrics {
            peak_global_stream_bytes: metrics.peak_global_stream_bytes,
            peak_task_stream_bytes: metrics.peak_task_stream_bytes,
        }
    }

    /// Returns the number of durability sync system calls exercised by this
    /// store instance. Benchmarks use observed I/O instead of inferred counts.
    pub fn durability_sync_calls(&self) -> u64 {
        self.inner.faults.sync_calls()
    }

    pub fn load(&self, task_id: &str) -> Result<TaskProjection, RuntimeError> {
        validate_task_id(task_id)?;
        recovery::ensure_task_loaded(
            &self.inner.tasks_root,
            &self.inner.catalog,
            &self.inner.projections,
            &self.inner.projection_load_lock,
            task_id,
        )?;
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

    /// Returns lightweight Task records without hydrating Chat or Tool details.
    pub fn list_task_records(&self) -> Vec<TaskRecord> {
        self.inner
            .catalog
            .read()
            .expect("Task catalog poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Strict collection reads preserve fail-closed behavior for Tasks whose
    /// journal had to be recovered while rebuilding a missing catalog.
    pub fn list_task_records_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        if let Some(error) = self
            .inner
            .projections
            .read()
            .expect("Task journal projections poisoned")
            .values()
            .find_map(|task| match task {
                RecoveredTask::Unavailable { error } => Some(error.clone()),
                RecoveredTask::Available { .. } => None,
            })
        {
            return Err(RuntimeError::Storage(error));
        }
        Ok(self.list_task_records())
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
        let projection = self.load(task_id)?;
        let committed_head = {
            projection
                .artifact_heads
                .get(artifact_id)
                .copied()
                .ok_or_else(|| {
                    RuntimeError::Storage(format!("Tool artifact is not committed: {artifact_id}"))
                })?
        };
        let artifact_key = (task_id.to_string(), artifact_id.to_string());
        let mut heads = self
            .inner
            .artifact_heads
            .lock()
            .expect("Tool artifact reconciliation lock poisoned");
        if let std::collections::hash_map::Entry::Vacant(entry) = heads.entry(artifact_key) {
            let reconciled = artifact::reconcile_one(
                &self.inner.tasks_root,
                task_id,
                artifact_id,
                committed_head,
            )?;
            entry.insert(reconciled);
        }
        let mut artifact =
            artifact::load(&self.inner.tasks_root, task_id, artifact_id, committed_head)?;
        artifact.revision = committed_head;
        Ok(artifact)
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

fn commit_batch(
    context: &CommitContext,
    task_id: &str,
    batch: &[QueuedWrite],
) -> Result<Option<CommittedTaskBatch>, RuntimeError> {
    let tasks_root = context.tasks_root.as_path();
    let catalog_records = context.catalog.as_ref();
    let projections = context.projections.as_ref();
    let projection_load_lock = context.projection_load_lock.as_ref();
    let artifact_heads = context.artifact_heads.as_ref();
    let faults = context.faults.as_ref();
    let replaced_artifact_ids = batch
        .iter()
        .flat_map(|queued| queued.write.artifacts.iter())
        .filter(|write| matches!(write.operation, ArtifactOperation::ReplaceDetails { .. }))
        .map(|write| write.artifact_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut reduced = reduce_batch(batch);
    let compaction = compaction::requested_compaction(batch);
    recovery::ensure_task_loaded(
        tasks_root,
        catalog_records,
        projections,
        projection_load_lock,
        task_id,
    )?;
    // The sole writer drafts one Task without holding the root projection lock,
    // so unrelated reads stay available during physical I/O.
    let current_task = projections
        .read()
        .expect("Task journal projections poisoned")
        .get(task_id)
        .cloned();
    remove_semantic_noops(current_task.as_ref(), &mut reduced.task_operations)?;
    if reduced.task_operations.is_empty()
        && reduced.artifacts.is_empty()
        && compaction == CompactionMode::None
    {
        require_available(current_task.as_ref(), task_id)?;
        return Ok(None);
    }
    let task_snapshot_changed = reduced.task_operations.iter().any(|operation| {
        matches!(
            operation,
            TaskOperation::Create { .. }
                | TaskOperation::ReplaceTask { .. }
                | TaskOperation::ReplaceProjection { .. }
                | TaskOperation::AppendText { .. }
                | TaskOperation::AppendMessage { .. }
                | TaskOperation::UpsertMessage { .. }
                | TaskOperation::ReplaceMessages { .. }
                | TaskOperation::ReplaceMessageMeta { .. }
        )
    });
    if reduced.task_operations.is_empty() && reduced.artifacts.is_empty() {
        let mut next_task =
            current_task.ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?;
        let compacted =
            match compaction::compact_task(tasks_root, &mut next_task, task_id, compaction, faults)
            {
                Ok(compacted) => compacted,
                Err(error) => {
                    quarantine_or_abort(tasks_root, task_id);
                    return Err(freeze_shared_task(projections, task_id, error));
                }
            };
        if compacted {
            recovery::publish_catalog(tasks_root, catalog_records, task_id, &next_task)?;
        }
        projections
            .write()
            .expect("Task journal projections poisoned")
            .insert(task_id.to_string(), next_task);
        return Ok(None);
    }
    let mut planned_artifacts = Vec::new();
    if !reduced.artifacts.is_empty() {
        let artifact_heads = match current_task.as_ref() {
            Some(RecoveredTask::Available { projection, .. }) => projection.artifact_heads.clone(),
            Some(RecoveredTask::Unavailable { error }) => {
                return Err(RuntimeError::Storage(error.clone()))
            }
            None => reduced
                .task_operations
                .iter()
                .find_map(|operation| match operation {
                    // Initial Tool details belong to the same atomic frame as
                    // Task creation, so their base heads come from its draft.
                    TaskOperation::Create { projection } => Some(projection.artifact_heads.clone()),
                    _ => None,
                })
                .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))?,
        };
        for (artifact_id, operations) in reduced.artifacts {
            let committed_head = artifact_heads
                .get(&artifact_id)
                .copied()
                .unwrap_or_default();
            artifact::validate_artifact_id(&artifact_id)?;
            let artifact_sequence = committed_head.checked_add(1).ok_or_else(|| {
                RuntimeError::Storage("Tool artifact sequence overflow".to_string())
            })?;
            reduced.task_operations.push(TaskOperation::CommitArtifact {
                artifact_id: artifact_id.clone(),
                artifact_sequence,
            });
            planned_artifacts.push((artifact_id, committed_head, operations));
        }
    }
    validate_operations(current_task.as_ref(), task_id, &reduced.task_operations)?;
    let sequence = match current_task.as_ref() {
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
    let journal = journal_path(tasks_root, task_id)?;
    if sequence == 1 {
        let task_dir = journal.parent().ok_or_else(|| {
            RuntimeError::Storage("Task journal has no parent directory".to_string())
        })?;
        failure::ensure_status(task_dir)?;
    }
    let frame = JournalFrame {
        format_version: 1,
        sequence,
        operations: reduced.task_operations,
    };
    // Reduce on a clone before I/O so publication after sync is an infallible
    // state swap, never a second path that can disagree with durable bytes.
    let mut next_state = HashMap::new();
    if let Some(current_task) = current_task {
        next_state.insert(task_id.to_string(), current_task);
    }
    apply_operations(&mut next_state, task_id, frame.operations.clone(), sequence)?;
    let mut next_task = next_state
        .remove(task_id)
        .expect("Task reducer must publish its target");
    let has_artifact_reference = !planned_artifacts.is_empty();
    let mut artifact_changes = Vec::new();
    for (artifact_id, committed_head, operations) in planned_artifacts {
        let mut artifact_heads = artifact_heads
            .lock()
            .expect("Tool artifact reconciliation lock poisoned");
        let artifact_key = (task_id.to_string(), artifact_id.clone());
        if !artifact_heads.contains_key(&artifact_key) {
            let reconciled =
                match artifact::reconcile_one(tasks_root, task_id, &artifact_id, committed_head) {
                    Ok(reconciled) => reconciled,
                    Err(error) => {
                        quarantine_or_abort(tasks_root, task_id);
                        return Err(freeze_shared_task(projections, task_id, error));
                    }
                };
            artifact_heads.insert(artifact_key.clone(), reconciled);
        }
        let reconciled_head = artifact_heads.get(&artifact_key).copied();
        let change = match artifact::prepare_reconciled_with_faults(
            tasks_root,
            task_id,
            &artifact_id,
            committed_head,
            reconciled_head,
            operations,
            faults,
        ) {
            Ok(change) => change,
            Err(error) => {
                quarantine_or_abort(tasks_root, task_id);
                return Err(freeze_shared_task(projections, task_id, error));
            }
        };
        artifact_heads.insert(artifact_key, change.artifact_sequence);
        artifact_changes.push(change);
    }
    let journal_kind = if has_artifact_reference {
        frame::JournalKind::ArtifactReference
    } else {
        frame::JournalKind::Task
    };
    let persisted = if sequence == 1 {
        frame::create_with_faults(&journal, &frame, journal_kind, faults)
    } else {
        frame::append_with_faults(&journal, &frame, journal_kind, faults)
    };
    if let Err(error) = persisted {
        quarantine_or_abort(tasks_root, task_id);
        return Err(freeze_shared_task(projections, task_id, error));
    }
    let compacted =
        match compaction::compact_task(tasks_root, &mut next_task, task_id, compaction, faults) {
            Ok(compacted) => compacted,
            Err(error) => {
                quarantine_or_abort(tasks_root, task_id);
                return Err(freeze_shared_task(projections, task_id, error));
            }
        };
    if let Err(error) = recovery::publish_catalog(tasks_root, catalog_records, task_id, &next_task)
    {
        quarantine_or_abort(tasks_root, task_id);
        return Err(freeze_shared_task(projections, task_id, error));
    }
    projections
        .write()
        .expect("Task journal projections poisoned")
        .insert(task_id.to_string(), next_task);
    Ok(Some(CommittedTaskBatch {
        task_id: task_id.to_string(),
        journal_sequence: if compacted { 1 } else { sequence },
        task_snapshot_changed,
        replaced_artifact_ids,
        artifact_changes,
    }))
}

fn quarantine_or_abort(tasks_root: &Path, task_id: &str) {
    let task_dir = tasks_root.join(task_id);
    if let Err(error) = failure::quarantine(&task_dir) {
        crate::logging::error(
            "task_journal_quarantine_failed",
            serde_json::json!({ "task_id": task_id, "error": error.to_string() }),
        );
        panic!("cannot quarantine Task after uncertain durability failure: {error}");
    }
}

fn remove_semantic_noops(
    task: Option<&RecoveredTask>,
    operations: &mut Vec<TaskOperation>,
) -> Result<(), RuntimeError> {
    let current = match task {
        Some(RecoveredTask::Available { projection, .. }) => Some(projection.as_ref()),
        Some(RecoveredTask::Unavailable { error }) => {
            return Err(RuntimeError::Storage(error.clone()))
        }
        None => None,
    };
    operations.retain(|operation| match operation {
        TaskOperation::AppendText { text, .. } => !text.is_empty(),
        TaskOperation::ReplaceTask { task } => current
            .map(|projection| !serialized_equal(&projection.task, task.as_ref()))
            .unwrap_or(true),
        TaskOperation::ReplaceProjection { projection } => current
            .map(|current| !serialized_equal(current, projection.as_ref()))
            .unwrap_or(true),
        TaskOperation::ReplaceMessageMeta { message_meta } => current
            .map(|projection| !serialized_equal(&projection.message_meta, message_meta.as_ref()))
            .unwrap_or(true),
        TaskOperation::AppendMessage { .. }
        | TaskOperation::UpsertMessage { .. }
        | TaskOperation::ReplaceMessages { .. }
        | TaskOperation::Create { .. }
        | TaskOperation::CommitArtifact { .. } => true,
    });
    Ok(())
}

fn serialized_equal<T: serde::Serialize>(left: &T, right: &T) -> bool {
    serde_json::to_vec(left).ok() == serde_json::to_vec(right).ok()
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

fn freeze_shared_task(
    projections: &RwLock<HashMap<String, RecoveredTask>>,
    task_id: &str,
    error: RuntimeError,
) -> RuntimeError {
    freeze_task(
        &mut projections
            .write()
            .expect("Task journal projections poisoned"),
        task_id,
        error,
    )
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
                TaskOperation::ReplaceProjection { projection } => {
                    task_operations.push(TaskOperation::ReplaceProjection {
                        projection: projection.clone(),
                    })
                }
                TaskOperation::AppendMessage { message } => {
                    task_operations.push(TaskOperation::AppendMessage {
                        message: message.clone(),
                    })
                }
                TaskOperation::UpsertMessage { message } => {
                    task_operations.push(TaskOperation::UpsertMessage {
                        message: message.clone(),
                    })
                }
                TaskOperation::ReplaceMessages {
                    messages,
                    message_meta,
                } => task_operations.push(TaskOperation::ReplaceMessages {
                    messages: messages.clone(),
                    message_meta: message_meta.clone(),
                }),
                TaskOperation::ReplaceMessageMeta { message_meta } => {
                    task_operations.push(TaskOperation::ReplaceMessageMeta {
                        message_meta: message_meta.clone(),
                    })
                }
                TaskOperation::CommitArtifact { .. } => {
                    unreachable!("artifact commit references are worker-owned")
                }
            }
        }
        for write in &queued.write.artifacts {
            match &write.operation {
                ArtifactOperation::ReplaceDetails { details } => {
                    artifacts
                        .entry(write.artifact_id.clone())
                        .or_default()
                        .push(ArtifactOperation::ReplaceDetails {
                            details: details.clone(),
                        });
                }
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
        replaced_artifact_ids: Vec::new(),
        artifact_changes: Vec::new(),
    })
}

fn require_available(task: Option<&RecoveredTask>, task_id: &str) -> Result<(), RuntimeError> {
    match task {
        Some(RecoveredTask::Available { .. }) => Ok(()),
        Some(RecoveredTask::Unavailable { error }) => Err(RuntimeError::Storage(error.clone())),
        None => Err(RuntimeError::TaskNotFound(task_id.to_string())),
    }
}

fn journal_path(tasks_root: &Path, task_id: &str) -> Result<PathBuf, RuntimeError> {
    validate_task_id(task_id)?;
    Ok(tasks_root.join(task_id).join(JOURNAL_FILE))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
