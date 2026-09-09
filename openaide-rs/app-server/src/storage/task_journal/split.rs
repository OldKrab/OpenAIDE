use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::protocol::errors::RuntimeError;
use crate::storage::atomic::{replace_file as durable_replace, sync_directory};
use crate::storage::records::{MessageMeta, StoredMessage, TaskRecord};

use super::frame::{self, FaultInjector, FramedRecord, JournalKind, ReplayedFrames};
use super::model::{JournalFrame, TaskOperation, TaskProjection};
use super::store::RecoveredTask;

pub(super) const TASK_FILE: &str = "task.json";
pub(super) const CHAT_SNAPSHOT_FILE: &str = "chat.snapshot";
pub(super) const CHAT_JOURNAL_FILE: &str = "chat.journal";
const TASK_SCHEMA_VERSION: u16 = 1;
pub(super) const CHAT_SCHEMA_VERSION: u16 = 2;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DurableTaskFile {
    schema_version: u16,
    storage_sequence: u64,
    chat_sequence: u64,
    chat_snapshot: String,
    chat_journal: String,
    /// Optional for stores written before interrupted Chat commits were detected
    /// at Navigation startup. A length check avoids replaying healthy cold Chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chat_journal_bytes: Option<u64>,
    task: TaskRecord,
}

pub(super) struct TaskMetadata {
    pub(super) task: TaskRecord,
    pub(super) storage_sequence: u64,
    pub(super) chat_sequence: u64,
    pub(super) chat_snapshot: String,
    pub(super) chat_journal: String,
    chat_journal_bytes: Option<u64>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatSnapshotFile {
    schema_version: u16,
    messages: Vec<StoredMessage>,
    message_meta: MessageMeta,
    artifact_heads: HashMap<String, u64>,
}

#[derive(Deserialize, Serialize)]
struct ChatFrame {
    #[serde(flatten)]
    journal: JournalFrame,
    #[serde(default)]
    task_patch: TaskMetadataPatch,
}

impl FramedRecord for ChatFrame {
    fn decode(payload: &[u8]) -> Result<Self, RuntimeError> {
        // Keep released Chat migrations at their existing boundary. The added
        // recovery facts are optional on all previously written journal frames.
        let journal = JournalFrame::decode(payload)?;
        let mut value: Value = serde_json::from_slice(payload)?;
        let task_patch = match value
            .as_object_mut()
            .and_then(|fields| fields.remove("task_patch"))
        {
            Some(patch) => serde_json::from_value(patch)?,
            None => TaskMetadataPatch::default(),
        };
        Ok(Self {
            journal,
            task_patch,
        })
    }

    fn format_version(&self) -> u16 {
        self.journal.format_version
    }

    fn sequence(&self) -> u64 {
        self.journal.sequence
    }
}

/// Only Task fields changed by this Chat commit need a recovery copy. Repeating
/// complete metadata would journal queued Images and Composer History on every
/// Agent chunk. Removed fields stay distinct from fields explicitly set to null.
#[derive(Default, Deserialize, Serialize)]
struct TaskMetadataPatch {
    #[serde(default)]
    fields: Map<String, Value>,
    #[serde(default)]
    removed_fields: Vec<String>,
}

impl TaskMetadataPatch {
    fn between(before: &TaskRecord, after: &TaskRecord) -> Result<Self, RuntimeError> {
        let before = serde_json::to_value(before)?;
        let mut after = after.clone();
        after.clear_process_local_agent_state();
        let after = serde_json::to_value(after)?;
        let before = before
            .as_object()
            .expect("Task metadata serializes as an object");
        let after = after
            .as_object()
            .expect("Task metadata serializes as an object");
        Ok(Self {
            fields: after
                .iter()
                .filter(|(key, value)| before.get(*key) != Some(*value))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            removed_fields: before
                .keys()
                .filter(|key| !after.contains_key(*key))
                .cloned()
                .collect(),
        })
    }

    fn apply(self, task: &mut TaskRecord) -> Result<(), RuntimeError> {
        if self.fields.is_empty() && self.removed_fields.is_empty() {
            return Ok(());
        }
        let mut value = serde_json::to_value(&*task)?;
        let fields = value
            .as_object_mut()
            .expect("Task metadata serializes as an object");
        for key in self.removed_fields {
            fields.remove(&key);
        }
        fields.extend(self.fields);
        let recovered: TaskRecord = serde_json::from_value(value)?;
        if recovered.task_id != task.task_id {
            return Err(RuntimeError::Storage(
                "Chat recovery metadata changes Task identity".to_string(),
            ));
        }
        *task = recovered;
        Ok(())
    }
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
    let mut file: DurableTaskFile = serde_json::from_slice(&bytes)?;
    // Older releases persisted option catalogs. Never hydrate those as live controls.
    file.task.config_options_catalog = None;
    file.task.config_mutation.preferences = None;
    validate_schema("Task metadata", file.schema_version, TASK_SCHEMA_VERSION)?;
    Ok(Some(TaskMetadata {
        task: file.task,
        storage_sequence: file.storage_sequence,
        chat_sequence: file.chat_sequence,
        chat_snapshot: file.chat_snapshot,
        chat_journal: file.chat_journal,
        chat_journal_bytes: file.chat_journal_bytes,
    }))
}

/// Startup must recover Send promotion and queue consumption before a metadata
/// reader can dispose a Prepared Task or offer accepted work for delivery again.
/// Prior files without a checkpoint retain their existing lazy hydration path.
pub(super) fn load_catalog_task(task_dir: &Path) -> Result<Option<TaskMetadata>, RuntimeError> {
    let Some(metadata) = load_task(task_dir)? else {
        return Ok(None);
    };
    if let Some(committed_bytes) = metadata.chat_journal_bytes {
        let journal_bytes = file_length_or_zero(&task_dir.join(&metadata.chat_journal))?;
        if journal_bytes > committed_bytes {
            let started = std::time::Instant::now();
            crate::logging::info(
                "task_metadata_recovery_started",
                serde_json::json!({ "task_id": metadata.task.task_id, "chat_sequence": metadata.chat_sequence }),
            );
            let recovery = load_projection(task_dir);
            crate::logging::info(
                "task_metadata_recovery_finished",
                serde_json::json!({
                    "task_id": metadata.task.task_id,
                    "outcome": if recovery.is_ok() { "recovered" } else { "unavailable" },
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
            recovery?;
            return load_task(task_dir);
        }
    }
    Ok(Some(metadata))
}

pub(super) fn load_projection(
    task_dir: &Path,
) -> Result<Option<(TaskProjection, u64)>, RuntimeError> {
    let Some(metadata) = load_task(task_dir)? else {
        return Ok(None);
    };
    let snapshot_path = task_dir.join(&metadata.chat_snapshot);
    let (snapshot, migrated_snapshot) = load_chat_snapshot(&snapshot_path)?;
    let mut projection = TaskProjection {
        task: metadata.task.clone(),
        messages: snapshot.messages.clone(),
        message_meta: snapshot.message_meta.clone(),
        artifact_heads: snapshot.artifact_heads.clone(),
    };
    let journal = task_dir.join(&metadata.chat_journal);
    if metadata.chat_sequence == 0
        && journal.is_file()
        && fs::metadata(&journal)?.len() < frame::FILE_HEADER_LEN as u64
    {
        // A crash can interrupt creation before even the first frame header.
        // No committed reference exists, so this empty generation is disposable.
        fs::remove_file(&journal)?;
        sync_directory(task_dir)?;
    }
    if journal.is_file() {
        let replayed: ReplayedFrames<ChatFrame> = frame::replay(&journal)?;
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
        for ChatFrame {
            journal: frame,
            task_patch,
        } in replayed.frames
        {
            let is_checkpoint = frame.sequence == metadata.chat_sequence;
            let needs_recovery = frame.sequence > metadata.chat_sequence;
            super::projection::apply_operations(
                &mut state,
                &task_id,
                frame.operations,
                frame.sequence,
            )?;
            if is_checkpoint || needs_recovery {
                // Later metadata-only changes (Archive, title, queue edits) own
                // their facts even when an older Chat frame contains Task state.
                let RecoveredTask::Available { projection, .. } =
                    state.get_mut(&task_id).expect("split replay retains Task")
                else {
                    unreachable!("split replay cannot quarantine Task")
                };
                if is_checkpoint {
                    projection.task = metadata.task.clone();
                } else {
                    task_patch.apply(&mut projection.task)?;
                }
            }
        }
        let RecoveredTask::Available {
            projection: replayed,
            ..
        } = state.remove(&task_id).expect("split replay retains Task")
        else {
            unreachable!("split replay cannot quarantine Task")
        };
        projection = *replayed;
        projection.task.message_history_version = projection.message_meta.version;
        let journal_bytes = if recovered_chat_sequence == 0 {
            // A complete file header can survive an interrupted first frame.
            // The next first append uses create_new, so an empty generation must
            // disappear just like a partially written header above.
            fs::remove_file(&journal)?;
            sync_directory(task_dir)?;
            0
        } else {
            fs::metadata(&journal)?.len()
        };
        if recovered_chat_sequence > metadata.chat_sequence
            || metadata
                .chat_journal_bytes
                .is_some_and(|bytes| bytes != journal_bytes)
        {
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
            if migrated_snapshot {
                durable_replace_json(
                    &snapshot_path,
                    &snapshot,
                    JournalKind::Task,
                    &FaultInjector::disabled(),
                )?;
            }
            return Ok(Some((projection, repaired_storage_sequence)));
        }
    } else if metadata.chat_sequence > 0 {
        return Err(RuntimeError::Storage(
            "Committed Chat journal generation is missing".to_string(),
        ));
    }
    if migrated_snapshot {
        durable_replace_json(
            &snapshot_path,
            &snapshot,
            JournalKind::Task,
            &FaultInjector::disabled(),
        )?;
    }
    if metadata.chat_journal_bytes.is_none() {
        // Upgrade at the normal hydration seam before any new Chat write can
        // enter the worker. A crash after its first new frame must leave an
        // already-durable old-byte checkpoint for metadata-only startup readers.
        publish_task(
            task_dir,
            &projection.task,
            metadata.storage_sequence,
            metadata.chat_sequence,
            &metadata.chat_snapshot,
            &metadata.chat_journal,
            &FaultInjector::disabled(),
        )?;
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
        schema_version: CHAT_SCHEMA_VERSION,
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
        let frame = ChatFrame {
            journal: JournalFrame {
                format_version: 1,
                schema_version: CHAT_SCHEMA_VERSION,
                sequence: next,
                operations: chat_operations,
            },
            task_patch: TaskMetadataPatch::between(&metadata.task, &projection.task)?,
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
            schema_version: CHAT_SCHEMA_VERSION,
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
    Ok(journal_bytes >= snapshot_bytes && journal_bytes >= 1024 * 1024)
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
            schema_version: TASK_SCHEMA_VERSION,
            storage_sequence,
            chat_sequence,
            chat_snapshot: chat_snapshot.to_string(),
            chat_journal: chat_journal.to_string(),
            chat_journal_bytes: Some(file_length_or_zero(&task_dir.join(chat_journal))?),
            task,
        },
        JournalKind::Task,
        faults,
    )
}

fn file_length_or_zero(path: &Path) -> Result<u64, RuntimeError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
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
        durable_replace(&temporary, path)?;
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

fn remove_if_present(path: &Path) -> Result<(), RuntimeError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn load_chat_snapshot(path: &Path) -> Result<(ChatSnapshotFile, bool), RuntimeError> {
    let bytes = fs::read(path)?;
    let mut value: Value = serde_json::from_slice(&bytes)?;
    let version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .and_then(|version| u16::try_from(version).ok())
        .ok_or_else(|| {
            RuntimeError::Storage("Chat snapshot schema version is missing".to_string())
        })?;
    let migrated = match version {
        CHAT_SCHEMA_VERSION => false,
        1 => {
            migrate_v1_tool_presentations(&mut value);
            value["schemaVersion"] = Value::from(CHAT_SCHEMA_VERSION);
            true
        }
        _ => {
            return Err(RuntimeError::Storage(format!(
                "Unsupported Chat snapshot schema version {version}"
            )))
        }
    };
    let snapshot: ChatSnapshotFile = serde_json::from_value(value)?;
    validate_schema(
        "Chat snapshot",
        snapshot.schema_version,
        CHAT_SCHEMA_VERSION,
    )?;
    Ok((snapshot, migrated))
}

/// Converts the only released v1 Chat shape that cannot deserialize into v2.
/// Migration stays at the durable boundary so runtime model evolution does not
/// silently redefine already-written Task history.
pub(super) fn migrate_v1_tool_presentations(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                migrate_v1_tool_presentations(value);
            }
        }
        Value::Object(fields) => {
            if let Some(Value::Object(presentation)) = fields.get_mut("presentation") {
                if !presentation.contains_key("actions") {
                    let kind = presentation.remove("kind");
                    let subjects = presentation.remove("subjects");
                    let actions = match (&kind, &subjects) {
                        (Some(Value::String(kind)), Some(Value::Array(subjects)))
                            if kind == "search" =>
                        {
                            // V1 flattened each query and scope into one display subject.
                            // Preserve that text without inventing scope or path-search facts.
                            Some(Value::Array(
                                subjects
                                    .iter()
                                    .map(|query| {
                                        serde_json::json!({
                                            "kind": "search",
                                            "query": query,
                                            "scopes": [],
                                            "target": "contents",
                                        })
                                    })
                                    .collect(),
                            ))
                        }
                        (Some(kind), Some(subjects)) => {
                            Some(Value::Array(vec![serde_json::json!({
                                "kind": kind,
                                "subjects": subjects,
                            })]))
                        }
                        _ => None,
                    };
                    if let Some(actions) = actions {
                        presentation.insert("actions".to_string(), actions);
                    }
                }
            }
            for value in fields.values_mut() {
                migrate_v1_tool_presentations(value);
            }
        }
        _ => {}
    }
}

fn validate_schema(kind: &str, version: u16, current: u16) -> Result<(), RuntimeError> {
    if version == current {
        Ok(())
    } else {
        Err(RuntimeError::Storage(format!(
            "Unsupported {kind} schema version {version}"
        )))
    }
}
