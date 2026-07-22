use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::storage::records::{MessageMeta, StoredMessage, TaskRecord};

use super::frame::{self, FaultInjector, JournalKind, ReplayedFrames};
use super::model::{JournalFrame, TaskOperation, TaskProjection};
use super::store::RecoveredTask;

pub(super) const TASK_FILE: &str = "task.json";
pub(super) const CHAT_SNAPSHOT_FILE: &str = "chat.snapshot";
pub(super) const CHAT_JOURNAL_FILE: &str = "chat.journal";
const SCHEMA_VERSION: u16 = 1;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DurableTaskFile {
    schema_version: u16,
    storage_sequence: u64,
    chat_sequence: u64,
    chat_snapshot: String,
    chat_journal: String,
    task: TaskRecord,
}

pub(super) struct TaskMetadata {
    pub(super) task: TaskRecord,
    pub(super) storage_sequence: u64,
    pub(super) chat_sequence: u64,
    pub(super) chat_snapshot: String,
    pub(super) chat_journal: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatSnapshotFile {
    schema_version: u16,
    messages: Vec<StoredMessage>,
    message_meta: MessageMeta,
    artifact_heads: HashMap<String, u64>,
}

pub(super) fn exists(task_dir: &Path) -> bool {
    matches!(load_task(task_dir), Ok(Some(metadata)) if task_dir.join(&metadata.chat_snapshot).is_file())
}

pub(super) fn load_task(task_dir: &Path) -> Result<Option<TaskMetadata>, RuntimeError> {
    let bytes = match fs::read(task_dir.join(TASK_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let file: DurableTaskFile = serde_json::from_slice(&bytes)?;
    validate_schema(file.schema_version)?;
    Ok(Some(TaskMetadata {
        task: file.task,
        storage_sequence: file.storage_sequence,
        chat_sequence: file.chat_sequence,
        chat_snapshot: file.chat_snapshot,
        chat_journal: file.chat_journal,
    }))
}

pub(super) fn load_projection(
    task_dir: &Path,
) -> Result<Option<(TaskProjection, u64)>, RuntimeError> {
    let Some(metadata) = load_task(task_dir)? else {
        return Ok(None);
    };
    let snapshot: ChatSnapshotFile =
        serde_json::from_slice(&fs::read(task_dir.join(&metadata.chat_snapshot))?)?;
    validate_schema(snapshot.schema_version)?;
    let mut projection = TaskProjection {
        task: metadata.task.clone(),
        messages: snapshot.messages,
        message_meta: snapshot.message_meta,
        artifact_heads: snapshot.artifact_heads,
    };
    let journal = task_dir.join(&metadata.chat_journal);
    if journal.is_file() {
        let replayed: ReplayedFrames<JournalFrame> = frame::replay(&journal)?;
        let recovered_chat_sequence = replayed.frame_count as u64;
        if recovered_chat_sequence < metadata.chat_sequence {
            return Err(RuntimeError::Storage(
                "Chat journal is behind its committed Task metadata".to_string(),
            ));
        }
        let task_id = metadata.task.task_id.clone();
        let mut state = HashMap::from([(
            task_id.clone(),
            RecoveredTask::Available {
                projection: Box::new(projection),
                journal_sequence: 0,
            },
        )]);
        for frame in replayed.frames {
            super::projection::apply_operations(
                &mut state,
                &task_id,
                frame.operations,
                frame.sequence,
            )?;
        }
        let RecoveredTask::Available {
            projection: replayed,
            ..
        } = state.remove(&task_id).expect("split replay retains Task")
        else {
            unreachable!("split replay cannot quarantine Task")
        };
        projection = *replayed;
        projection.task = metadata.task;
        projection.task.message_history_version = projection.message_meta.version;
        if recovered_chat_sequence > metadata.chat_sequence {
            let repaired_storage_sequence = metadata
                .storage_sequence
                .saturating_add(recovered_chat_sequence - metadata.chat_sequence);
            publish_task(
                task_dir,
                &projection.task,
                repaired_storage_sequence,
                recovered_chat_sequence,
                &metadata.chat_snapshot,
                &metadata.chat_journal,
                &FaultInjector::disabled(),
            )?;
            return Ok(Some((projection, repaired_storage_sequence)));
        }
    } else if metadata.chat_sequence > 0 {
        return Err(RuntimeError::Storage(
            "Committed Chat journal generation is missing".to_string(),
        ));
    }
    Ok(Some((projection, metadata.storage_sequence)))
}

/// Publishes the initial Chat snapshot before metadata becomes discoverable.
pub(super) fn publish_initial(
    task_dir: &Path,
    projection: &TaskProjection,
    storage_sequence: u64,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    fs::create_dir_all(task_dir)?;
    let snapshot = ChatSnapshotFile {
        schema_version: SCHEMA_VERSION,
        messages: projection.messages.clone(),
        message_meta: projection.message_meta.clone(),
        artifact_heads: projection.artifact_heads.clone(),
    };
    durable_replace_json(
        &task_dir.join(CHAT_SNAPSHOT_FILE),
        &snapshot,
        JournalKind::Task,
        faults,
    )?;
    publish_task(
        task_dir,
        &projection.task,
        storage_sequence,
        0,
        CHAT_SNAPSHOT_FILE,
        CHAT_JOURNAL_FILE,
        faults,
    )
}

/// Appends only Chat-affecting operations, then publishes compact Task metadata.
pub(super) fn append(
    task_dir: &Path,
    projection: &TaskProjection,
    operations: &[TaskOperation],
    storage_sequence: u64,
    journal_kind: JournalKind,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let metadata = load_task(task_dir)?
        .ok_or_else(|| RuntimeError::Storage("Split Task metadata is missing".to_string()))?;
    let chat_operations = durable_chat_operations(operations);
    let next_chat_sequence = if chat_operations.is_empty() {
        metadata.chat_sequence
    } else {
        let next = metadata
            .chat_sequence
            .checked_add(1)
            .ok_or_else(|| RuntimeError::Storage("Chat journal sequence overflow".to_string()))?;
        let frame = JournalFrame {
            format_version: 1,
            sequence: next,
            operations: chat_operations,
        };
        let journal = task_dir.join(&metadata.chat_journal);
        if next == 1 {
            frame::create_with_faults(&journal, &frame, journal_kind, faults)?;
        } else {
            frame::append_with_faults(&journal, &frame, journal_kind, faults)?;
        }
        next
    };
    publish_task(
        task_dir,
        &projection.task,
        storage_sequence,
        next_chat_sequence,
        &metadata.chat_snapshot,
        &metadata.chat_journal,
        faults,
    )
}

/// Switches metadata to a new snapshot generation before removing obsolete bytes.
pub(super) fn compact(
    task_dir: &Path,
    projection: &TaskProjection,
    storage_sequence: u64,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let metadata = load_task(task_dir)?
        .ok_or_else(|| RuntimeError::Storage("Split Task metadata is missing".to_string()))?;
    let generation = uuid::Uuid::new_v4();
    let new_snapshot = format!("{CHAT_SNAPSHOT_FILE}.{generation}");
    let new_journal = format!("{CHAT_JOURNAL_FILE}.{generation}");
    durable_replace_json(
        &task_dir.join(&new_snapshot),
        &ChatSnapshotFile {
            schema_version: SCHEMA_VERSION,
            messages: projection.messages.clone(),
            message_meta: projection.message_meta.clone(),
            artifact_heads: projection.artifact_heads.clone(),
        },
        JournalKind::Compaction,
        faults,
    )?;
    faults.check(
        JournalKind::Compaction,
        super::frame::FaultPoint::CompactionValidate,
    )?;
    faults.check(
        JournalKind::Compaction,
        super::frame::FaultPoint::CompactionPublish,
    )?;
    publish_task(
        task_dir,
        &projection.task,
        storage_sequence,
        0,
        &new_snapshot,
        &new_journal,
        faults,
    )?;
    remove_if_present(&task_dir.join(metadata.chat_journal))?;
    if metadata.chat_snapshot != new_snapshot {
        remove_if_present(&task_dir.join(metadata.chat_snapshot))?;
    }
    faults.check(
        JournalKind::Compaction,
        super::frame::FaultPoint::CompactionPublishParentSync,
    )?;
    sync_directory(task_dir)
}

pub(super) fn compaction_is_worthwhile(task_dir: &Path) -> Result<bool, RuntimeError> {
    let metadata = load_task(task_dir)?
        .ok_or_else(|| RuntimeError::Storage("Split Task metadata is missing".to_string()))?;
    let journal = task_dir.join(&metadata.chat_journal);
    if !journal.exists() {
        return Ok(false);
    }
    let replayed: ReplayedFrames<JournalFrame> = frame::scan(&journal)?;
    if replayed.frame_count >= 128 {
        return Ok(true);
    }
    let journal_bytes = fs::metadata(journal)?.len();
    let snapshot_bytes = fs::metadata(task_dir.join(metadata.chat_snapshot))?.len();
    Ok(journal_bytes >= snapshot_bytes && journal_bytes >= 64 * 1024)
}

pub(super) fn migrate(
    task_dir: &Path,
    projection: &TaskProjection,
    storage_sequence: u64,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    publish_initial(task_dir, projection, storage_sequence, faults)?;
    let (verified, verified_sequence) = load_projection(task_dir)?.ok_or_else(|| {
        RuntimeError::Storage("Published split Task could not be loaded".to_string())
    })?;
    let mut expected = projection.clone();
    expected.task.clear_process_local_agent_state();
    if serde_json::to_value(&verified)? != serde_json::to_value(&expected)?
        || verified_sequence != storage_sequence
    {
        return Err(RuntimeError::Storage(
            "Published split Task failed validation".to_string(),
        ));
    }
    if let Err(error) = remove_legacy_files(task_dir) {
        // The split files are already validated and authoritative. Cleanup is
        // retryable on the next startup and must not make the Task unreadable.
        crate::logging::warn(
            "legacy_task_cleanup_failed",
            serde_json::json!({
                "task_id": projection.task.task_id,
                "error": error.to_string(),
            }),
        );
    }
    Ok(())
}

pub(super) fn remove_legacy_files(task_dir: &Path) -> Result<(), RuntimeError> {
    for name in [super::store::JOURNAL_FILE, "task.catalog.json"] {
        match fs::remove_file(task_dir.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    sync_directory(task_dir)
}

fn durable_chat_operations(operations: &[TaskOperation]) -> Vec<TaskOperation> {
    operations
        .iter()
        .filter_map(|operation| match operation {
            TaskOperation::Create { .. } | TaskOperation::ReplaceTask { .. } => None,
            TaskOperation::ReplaceProjection { projection } => {
                let mut projection = projection.as_ref().clone();
                projection.task.clear_process_local_agent_state();
                Some(TaskOperation::ReplaceProjection {
                    projection: Box::new(projection),
                })
            }
            operation => Some(operation.clone()),
        })
        .collect()
}

fn publish_task(
    task_dir: &Path,
    task: &TaskRecord,
    storage_sequence: u64,
    chat_sequence: u64,
    chat_snapshot: &str,
    chat_journal: &str,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let mut task = task.clone();
    task.clear_process_local_agent_state();
    durable_replace_json(
        &task_dir.join(TASK_FILE),
        &DurableTaskFile {
            schema_version: SCHEMA_VERSION,
            storage_sequence,
            chat_sequence,
            chat_snapshot: chat_snapshot.to_string(),
            chat_journal: chat_journal.to_string(),
            task,
        },
        JournalKind::Task,
        faults,
    )
}

fn durable_replace_json<T: Serialize>(
    path: &Path,
    value: &T,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec(value)?;
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeError::Storage("split Task file has no parent".to_string()))?;
    let temporary = temporary_path(path);
    let result = (|| {
        faults.check(kind, super::frame::FaultPoint::DirectoryParentSync)?;
        faults.check(kind, super::frame::FaultPoint::CreateOpen)?;
        faults.check(kind, super::frame::FaultPoint::AppendOpen)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        faults.check(kind, super::frame::FaultPoint::CreateHeaderWrite)?;
        faults.check(kind, super::frame::FaultPoint::FrameLengthWrite)?;
        faults.check(kind, super::frame::FaultPoint::FramePayloadWrite)?;
        file.write_all(&bytes)?;
        faults.check(kind, super::frame::FaultPoint::FrameChecksumWrite)?;
        faults.check(kind, super::frame::FaultPoint::FileSync)?;
        faults.record_sync();
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        faults.check(kind, super::frame::FaultPoint::ParentSync)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("task");
    path.with_file_name(format!(".{name}.{}", uuid::Uuid::new_v4()))
}

fn sync_directory(path: &Path) -> Result<(), RuntimeError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<(), RuntimeError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_schema(version: u16) -> Result<(), RuntimeError> {
    if version == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(RuntimeError::Storage(format!(
            "Unsupported split Task schema version {version}"
        )))
    }
}
