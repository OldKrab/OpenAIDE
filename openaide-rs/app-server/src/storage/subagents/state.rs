//! The Subagent catalogs and open histories that the Task journal worker writes
//! (ADR-0076). The worker applies a batch in memory and writes each changed
//! history once, so a streamed chunk costs neither a reload nor its own sync.
//! Readers see committed state; one lock also serializes every disk access.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use crate::protocol::errors::RuntimeError;

use super::history::{
    append_history_frame, durable_create_empty, encode_change_frame, journal_budget,
    load_upgraded_history, HistoryChange, HistoryFile, COMPACT_AFTER_FRAMES, HISTORY_JOURNAL_FILE,
    HISTORY_SCHEMA_VERSION, HISTORY_SNAPSHOT_FILE,
};
use super::writes::{apply_to_history, apply_to_record, SubagentOperation, SubagentWrite};
use super::{
    committed_change, load_catalog, write_catalog, CatalogFile, CommittedSubagentChange,
    SubagentCatalogProjection, SubagentRecord, SubagentStatusRecord, SUBAGENTS_DIR,
};

#[derive(Default)]
pub(crate) struct SubagentState {
    tasks: Mutex<HashMap<String, TaskSubagents>>,
}

/// One Task's catalog and the histories its live Subagents write. A history
/// leaves memory when its Subagent stops.
// TODO: Drop a Task's catalog from memory once its last Subagent stops, so a
// long-running App Server does not keep one catalog per Task it ever ran.
struct TaskSubagents {
    catalog: CatalogFile,
    histories: HashMap<String, OpenHistory>,
}

struct OpenHistory {
    file: HistoryFile,
    snapshot_bytes: u64,
    journal_bytes: u64,
}

/// What one worker batch committed, for publication.
#[derive(Debug, Clone)]
pub struct CommittedSubagentBatch {
    /// Every committed history revision, in commit order.
    pub revisions: Vec<CommittedSubagentRevision>,
    /// The catalog after the batch.
    pub catalog: SubagentCatalogProjection,
}

#[derive(Debug, Clone)]
pub struct CommittedSubagentRevision {
    pub subagent_id: String,
    pub revision: u64,
    /// `None` when the revision cannot be described as a change, so a watching
    /// client meets a revision gap and re-reads the history.
    pub change: Option<CommittedSubagentChange>,
}

/// Frames and outcomes waiting for one history's single write in a batch.
#[derive(Default)]
struct PendingHistory {
    frames: Vec<u8>,
    snapshot: bool,
    writes: Vec<usize>,
    revisions: Vec<usize>,
}

impl SubagentState {
    /// Applies `writes` in order and writes every changed history and the catalog
    /// once. Returns what to publish and one outcome per write. A failed history
    /// becomes unavailable without affecting the Task or other histories.
    pub(crate) fn commit_batch(
        &self,
        tasks_root: &Path,
        task_id: &str,
        writes: &[&SubagentWrite],
    ) -> (
        Option<CommittedSubagentBatch>,
        Vec<Result<(), RuntimeError>>,
    ) {
        let started_at = std::time::Instant::now();
        let mut tasks = self.tasks.lock().expect("Subagent state poisoned");
        let task_dir = tasks_root.join(task_id);
        let root = task_dir.join(SUBAGENTS_DIR);
        let task = match cached_task(&mut tasks, &root, task_id) {
            Ok(task) => task,
            Err(error) => return (None, writes.iter().map(|_| Err(error.clone())).collect()),
        };
        let mut batch = BatchState::default();
        let mut outcomes = Vec::with_capacity(writes.len());
        for (index, write) in writes.iter().enumerate() {
            let outcome = apply_write(task, &task_dir, &root, write, index, &mut batch);
            outcomes.push(outcome);
        }
        let compacted = persist_histories(task, &root, task_id, &mut batch, &mut outcomes);
        if batch.catalog_persist {
            if let Err(error) = write_catalog(&root, &task.catalog) {
                log_failure(task_id, None, "catalog", &error);
                // The disk keeps the last catalog it accepted, and memory follows
                // it. The revision stays above every published one, because a
                // client ignores an older catalog; histories reload from disk.
                let revision = task.catalog.revision;
                match load_catalog(&root) {
                    Ok(catalog) => {
                        task.catalog = CatalogFile {
                            revision,
                            ..catalog
                        };
                        task.histories.clear();
                    }
                    Err(_) => {
                        tasks.remove(task_id);
                    }
                }
                return (None, writes.iter().map(|_| Err(error.clone())).collect());
            }
        }
        for subagent_id in &batch.stopped {
            task.histories.remove(subagent_id);
        }
        let elapsed = started_at.elapsed();
        if elapsed.as_millis() >= 50 {
            crate::logging::warn(
                "subagent_batch_commit_slow",
                serde_json::json!({
                    "task_id": task_id,
                    "duration_ms": elapsed.as_millis(),
                    "write_count": writes.len(),
                    "revision_count": batch.revisions.len(),
                    "compacted_count": compacted,
                }),
            );
        }
        let committed = (batch.catalog_changed || !batch.revisions.is_empty()).then(|| {
            CommittedSubagentBatch {
                revisions: batch.revisions,
                catalog: SubagentCatalogProjection {
                    revision: task.catalog.revision,
                    entries: task.catalog.entries.clone(),
                },
            }
        });
        (committed, outcomes)
    }

    pub(super) fn catalog(
        &self,
        root: &Path,
        task_id: &str,
    ) -> Result<SubagentCatalogProjection, RuntimeError> {
        let tasks = self.tasks.lock().expect("Subagent state poisoned");
        let catalog = match tasks.get(task_id) {
            Some(task) => task.catalog.clone(),
            None => load_catalog(root)?,
        };
        Ok(SubagentCatalogProjection {
            revision: catalog.revision,
            entries: catalog.entries,
        })
    }

    /// Reads a committed history: from memory while its Subagent is live, else
    /// from disk. `require_available` refuses a history the catalog marks lost.
    pub(super) fn history(
        &self,
        root: &Path,
        task_id: &str,
        subagent_id: &str,
        require_available: bool,
    ) -> Result<HistoryFile, RuntimeError> {
        let tasks = self.tasks.lock().expect("Subagent state poisoned");
        let cached = tasks.get(task_id);
        if require_available {
            let disk_catalog;
            let catalog = match cached {
                Some(task) => &task.catalog,
                None => {
                    disk_catalog = load_catalog(root)?;
                    &disk_catalog
                }
            };
            let record = catalog
                .entries
                .iter()
                .find(|entry| entry.subagent_id == subagent_id)
                .ok_or_else(|| RuntimeError::TaskNotFound("subagent_id".to_string()))?;
            if !record.history_available {
                return Err(RuntimeError::Storage(
                    "Subagent history is unavailable".to_string(),
                ));
            }
        }
        if let Some(open) = cached.and_then(|task| task.histories.get(subagent_id)) {
            return Ok(open.file.clone());
        }
        let task_dir = root.parent().unwrap_or(root);
        Ok(load_upgraded_history(task_dir, &root.join(subagent_id))?.0)
    }

    /// Upgrades a history on disk unless a live Subagent holds it in memory,
    /// whose writer owns its files. Returns whether it was rewritten.
    pub(super) fn upgrade_unopened(&self, history_dir: &Path) -> Result<bool, RuntimeError> {
        let tasks = self.tasks.lock().expect("Subagent state poisoned");
        let subagent_root = history_dir.parent().unwrap_or(history_dir);
        let task_dir = subagent_root.parent().unwrap_or(subagent_root);
        let name = |path: &Path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        };
        let open = name(task_dir)
            .and_then(|task_id| tasks.get(&task_id))
            .zip(name(history_dir))
            .is_some_and(|(task, subagent_id)| task.histories.contains_key(&subagent_id));
        if open {
            return Ok(false);
        }
        Ok(load_upgraded_history(task_dir, history_dir)?.1)
    }

    /// Drops one Task's state after its files were removed.
    pub(crate) fn forget_task(&self, task_id: &str) {
        self.tasks
            .lock()
            .expect("Subagent state poisoned")
            .remove(task_id);
    }

    /// Drops every Task's state after all Task history was removed.
    pub(crate) fn clear(&self) {
        self.tasks.lock().expect("Subagent state poisoned").clear();
    }
}

#[derive(Default)]
struct BatchState {
    pending: HashMap<String, PendingHistory>,
    revisions: Vec<CommittedSubagentRevision>,
    /// The catalog changed in memory, so the batch publishes it.
    catalog_changed: bool,
    /// The catalog changed in a way worth a durable write: anything but a history
    /// revision, which the history itself records. The persisted catalog's
    /// revisions may therefore trail its histories until the next such write; a
    /// restarted App Server serves fresh baselines, so no client compares them.
    // TODO(ADR-0076): Take each entry's history revision from its stream when the
    // catalog loads, once the stream format makes that a metadata read; until then
    // a reloaded catalog can report an older history revision than its history.
    catalog_persist: bool,
    stopped: HashSet<String>,
    /// Changes dropped with a history that failed later in the batch.
    abandoned: Vec<PendingHistory>,
}

fn cached_task<'a>(
    tasks: &'a mut HashMap<String, TaskSubagents>,
    root: &Path,
    task_id: &str,
) -> Result<&'a mut TaskSubagents, RuntimeError> {
    if !tasks.contains_key(task_id) {
        let catalog = load_catalog(root)?;
        tasks.insert(
            task_id.to_string(),
            TaskSubagents {
                catalog,
                histories: HashMap::new(),
            },
        );
    }
    Ok(tasks.get_mut(task_id).expect("inserted above"))
}

fn apply_write(
    task: &mut TaskSubagents,
    task_dir: &Path,
    root: &Path,
    write: &SubagentWrite,
    index: usize,
    batch: &mut BatchState,
) -> Result<(), RuntimeError> {
    if let SubagentOperation::Spawn { .. } = &write.operation {
        return spawn(task, task_dir, root, write, batch);
    }
    let record = task
        .catalog
        .entries
        .iter_mut()
        .find(|entry| entry.native_session_id == write.native_session_id)
        .ok_or_else(|| RuntimeError::TaskNotFound("subagent native session".to_string()))?;
    if !write.changes_history() {
        apply_to_record(record, write.operation.clone());
        if is_terminal(record.status) {
            batch.stopped.insert(record.subagent_id.clone());
        } else {
            batch.stopped.remove(&record.subagent_id);
        }
        bump_catalog(&mut task.catalog.revision, batch, true);
        return Ok(());
    }
    let subagent_id = record.subagent_id.clone();
    let open = match task.histories.entry(subagent_id.clone()) {
        std::collections::hash_map::Entry::Occupied(open) => open.into_mut(),
        std::collections::hash_map::Entry::Vacant(vacant) => {
            match open_history(task_dir, &root.join(&subagent_id)) {
                Ok(open) => vacant.insert(open),
                Err(error) => {
                    if record.history_available {
                        record.history_available = false;
                        bump_catalog(&mut task.catalog.revision, batch, true);
                    }
                    return Err(error);
                }
            }
        }
    };
    let revision = match apply_history_write(open, task_dir, write, &subagent_id, batch) {
        Ok(revision) => revision,
        Err(error) => {
            // Memory may hold part of the change, so the next write reloads the
            // disk, and this batch's earlier changes to the history are not saved.
            task.histories.remove(&subagent_id);
            if let Some(abandoned) = batch.pending.remove(&subagent_id) {
                batch.abandoned.push(abandoned);
            }
            return Err(error);
        }
    };
    batch
        .pending
        .get_mut(&subagent_id)
        .expect("queued by apply_history_write")
        .writes
        .push(index);
    record.history_revision = revision;
    let mut persist = !record.history_available;
    record.history_available = true;
    if record.status == SubagentStatusRecord::WaitingForActivity {
        record.status = SubagentStatusRecord::Running;
        persist = true;
    }
    bump_catalog(&mut task.catalog.revision, batch, persist);
    Ok(())
}

/// Applies one history write to an open history, queues its frame or a
/// snapshot, and records the revision. Returns the new revision.
fn apply_history_write(
    open: &mut OpenHistory,
    task_dir: &Path,
    write: &SubagentWrite,
    subagent_id: &str,
    batch: &mut BatchState,
) -> Result<u64, RuntimeError> {
    let history = &mut open.file;
    let message_count = history.messages.len();
    let change = apply_to_history(history, write.operation.clone())?;
    // A changed message may carry new image bytes; they become files before the
    // frame or snapshot that references them (ADR-0075).
    if !matches!(change, HistoryChange::AppendText(_)) {
        crate::storage::chat_images::externalize_messages(task_dir, &mut history.messages)?;
    }
    let committed = committed_change(history, &change, message_count);
    history.revision = history.revision.saturating_add(1);
    history.journal_frames = history.journal_frames.saturating_add(1);
    let pending = batch.pending.entry(subagent_id.to_string()).or_default();
    if !pending.snapshot {
        // A schema 1 history is published once as schema 2 before its journal
        // takes a frame that schema 1 readers do not know.
        let frame = if history.schema_version == HISTORY_SCHEMA_VERSION
            && history.journal_frames < COMPACT_AFTER_FRAMES
        {
            encode_change_frame(history, change)?
        } else {
            None
        };
        match frame {
            Some(frame)
                if open.journal_bytes + (pending.frames.len() + frame.len()) as u64
                    <= journal_budget(open.snapshot_bytes) =>
            {
                pending.frames.extend_from_slice(&frame)
            }
            _ => {
                pending.snapshot = true;
                pending.frames.clear();
            }
        }
    }
    pending.revisions.push(batch.revisions.len());
    batch.revisions.push(CommittedSubagentRevision {
        subagent_id: subagent_id.to_string(),
        revision: history.revision,
        change: committed,
    });
    Ok(history.revision)
}

fn spawn(
    task: &mut TaskSubagents,
    task_dir: &Path,
    root: &Path,
    write: &SubagentWrite,
    batch: &mut BatchState,
) -> Result<(), RuntimeError> {
    let SubagentOperation::Spawn {
        parent_native_session_id,
        name,
        delegated_task,
        capabilities,
        details,
    } = write.operation.clone()
    else {
        unreachable!("checked by the caller");
    };
    let catalog = &mut task.catalog;
    if catalog
        .entries
        .iter()
        .any(|entry| entry.native_session_id == write.native_session_id)
    {
        return Ok(());
    }
    let parent_subagent_id = catalog
        .entries
        .iter()
        .find(|entry| entry.native_session_id == parent_native_session_id)
        .map(|entry| entry.subagent_id.clone());
    let subagent_id = format!("subagent_{}", uuid::Uuid::new_v4().simple());
    let mut history = HistoryFile::default();
    history.message_meta.task_id = task_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let history_dir = root.join(&subagent_id);
    crate::storage::atomic::write_json(&history_dir.join(HISTORY_SNAPSHOT_FILE), &history)?;
    durable_create_empty(&history_dir.join(HISTORY_JOURNAL_FILE))?;
    let snapshot_bytes = fs::metadata(history_dir.join(HISTORY_SNAPSHOT_FILE))?.len();
    task.histories.insert(
        subagent_id.clone(),
        OpenHistory {
            file: history,
            snapshot_bytes,
            journal_bytes: 0,
        },
    );
    catalog.next_spawn_order = catalog.next_spawn_order.saturating_add(1);
    catalog.entries.push(SubagentRecord {
        subagent_id,
        native_session_id: write.native_session_id.clone(),
        parent_native_session_id,
        parent_subagent_id,
        name,
        delegated_task,
        status: SubagentStatusRecord::WaitingForActivity,
        capabilities,
        spawned_order: catalog.next_spawn_order,
        history_revision: 0,
        history_available: true,
        details,
    });
    bump_catalog(&mut catalog.revision, batch, true);
    Ok(())
}

fn open_history(task_dir: &Path, history_dir: &Path) -> Result<OpenHistory, RuntimeError> {
    let (file, _) = load_upgraded_history(task_dir, history_dir)?;
    let bytes = |name| fs::metadata(history_dir.join(name)).map(|metadata| metadata.len());
    Ok(OpenHistory {
        file,
        snapshot_bytes: bytes(HISTORY_SNAPSHOT_FILE)?,
        journal_bytes: bytes(HISTORY_JOURNAL_FILE).unwrap_or_default(),
    })
}

/// Writes each changed history once: its frames with one sync, or a snapshot when
/// the journal is due for compaction. Returns how many histories were compacted.
fn persist_histories(
    task: &mut TaskSubagents,
    root: &Path,
    task_id: &str,
    batch: &mut BatchState,
    outcomes: &mut [Result<(), RuntimeError>],
) -> usize {
    let mut compacted = 0;
    let mut unpublished = HashSet::new();
    for abandoned in std::mem::take(&mut batch.abandoned) {
        for index in abandoned.writes {
            outcomes[index] = Err(RuntimeError::Storage(
                "Subagent history change was not saved".to_string(),
            ));
        }
        unpublished.extend(abandoned.revisions);
    }
    for (subagent_id, pending) in std::mem::take(&mut batch.pending) {
        let Some(open) = task.histories.get_mut(&subagent_id) else {
            continue;
        };
        let history_dir = root.join(&subagent_id);
        let written = if pending.snapshot {
            compacted += 1;
            write_snapshot(open, &history_dir)
        } else if pending.frames.is_empty() {
            Ok(())
        } else {
            append_history_frame(&history_dir.join(HISTORY_JOURNAL_FILE), &pending.frames)
                .map(|()| open.journal_bytes += pending.frames.len() as u64)
        };
        let Err(error) = written else {
            continue;
        };
        log_failure(task_id, Some(&subagent_id), "history", &error);
        task.histories.remove(&subagent_id);
        if let Some(record) = task
            .catalog
            .entries
            .iter_mut()
            .find(|entry| entry.subagent_id == subagent_id)
        {
            record.history_available = false;
            batch.catalog_persist = true;
        }
        for index in pending.writes {
            outcomes[index] = Err(error.clone());
        }
        unpublished.extend(pending.revisions);
    }
    if !unpublished.is_empty() {
        let mut index = 0;
        batch.revisions.retain(|_| {
            index += 1;
            !unpublished.contains(&(index - 1))
        });
    }
    compacted
}

fn write_snapshot(open: &mut OpenHistory, history_dir: &Path) -> Result<(), RuntimeError> {
    open.file.schema_version = HISTORY_SCHEMA_VERSION;
    open.file.journal_frames = 0;
    let snapshot_path = history_dir.join(HISTORY_SNAPSHOT_FILE);
    crate::storage::atomic::write_json(&snapshot_path, &open.file)?;
    durable_create_empty(&history_dir.join(HISTORY_JOURNAL_FILE))?;
    open.snapshot_bytes = fs::metadata(&snapshot_path)?.len();
    open.journal_bytes = 0;
    Ok(())
}

fn bump_catalog(revision: &mut u64, batch: &mut BatchState, persist: bool) {
    *revision = revision.saturating_add(1);
    batch.catalog_changed = true;
    batch.catalog_persist |= persist;
}

fn is_terminal(status: SubagentStatusRecord) -> bool {
    !matches!(
        status,
        SubagentStatusRecord::WaitingForActivity | SubagentStatusRecord::Running
    )
}

fn log_failure(task_id: &str, subagent_id: Option<&str>, file: &str, error: &RuntimeError) {
    crate::logging::warn(
        "subagent_write_failed",
        serde_json::json!({
            "task_id": task_id,
            "subagent_id": subagent_id,
            "file": file,
            "error_kind": error.reason(),
        }),
    );
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
