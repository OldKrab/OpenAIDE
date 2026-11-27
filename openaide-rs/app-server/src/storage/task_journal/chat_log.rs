//! The physical `chat.log` of one Task (ADR-0086, amending ADR-0029).
//!
//! One file holds a snapshot record followed by delta frames. Frame numbers
//! are monotonic for the life of the Task and never restart: compaction and
//! native replacement write a new file whose snapshot "covers through frame N"
//! and rename it over the old one, so the next frame is N + 1. The number of
//! the last frame is the Task's Chat version.
//!
//! Layout: `OAIDECL\0`, the format version (u16), the number of the last frame
//! the snapshot covers (u64), the snapshot record, then one checksummed frame
//! per Chat-changing transaction. Frames reuse the journal framing of
//! `frame.rs`. The covered frame number sits in the fixed header so a startup
//! check can count frames without decoding the snapshot.
//!
//! `task.json` owns the committed frame number (its checkpoint). This module
//! never reads it: callers pass the checkpoint where it limits what counts as
//! committed.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::protocol::errors::RuntimeError;
use crate::storage::records::{MessageMeta, StoredMessage, TaskRecord};

use super::frame::{self, FaultInjector, FaultPoint, FramedRecord, JournalKind};
use super::model::{JournalFrame, TaskOperation};

pub(super) const CHAT_LOG_FILE: &str = "chat.log";
/// The compacted log while it is written. Only the storage worker writes a
/// Task's log, so one fixed name suffices and startup removes an orphan.
pub(super) const CHAT_LOG_TEMP_FILE: &str = "chat.log.compact";
const LOG_MAGIC: &[u8; 8] = b"OAIDECL\0";
/// Version of the file layout above.
pub(super) const LOG_FORMAT_VERSION: u16 = 1;
const LOG_HEADER_LEN: usize = LOG_MAGIC.len() + size_of::<u16>() + size_of::<u64>();

/// The materialized Chat a snapshot record carries.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChatSnapshotFile {
    pub(super) schema_version: u16,
    pub(super) messages: Vec<StoredMessage>,
    pub(super) message_meta: MessageMeta,
    pub(super) artifact_heads: HashMap<String, u64>,
}

/// One delta frame: the normalized Chat operations of one transaction plus the
/// Task facts that transaction changed.
#[derive(Deserialize, Serialize)]
pub(super) struct ChatFrame {
    #[serde(flatten)]
    pub(super) journal: JournalFrame,
    #[serde(default)]
    pub(super) task_patch: TaskMetadataPatch,
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
pub(super) struct TaskMetadataPatch {
    #[serde(default)]
    fields: Map<String, Value>,
    #[serde(default)]
    removed_fields: Vec<String>,
}

impl TaskMetadataPatch {
    pub(super) fn between(before: &TaskRecord, after: &TaskRecord) -> Result<Self, RuntimeError> {
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

    pub(super) fn apply(self, task: &mut TaskRecord) -> Result<(), RuntimeError> {
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

pub(super) fn path(task_dir: &Path) -> PathBuf {
    task_dir.join(CHAT_LOG_FILE)
}

pub(super) fn exists(task_dir: &Path) -> bool {
    path(task_dir).is_file()
}

/// Removes a compacted log a crash left before its rename. It was never
/// referenced by anything, so removal cannot lose data.
pub(super) fn remove_orphaned_temp(task_dir: &Path) {
    match fs::remove_file(task_dir.join(CHAT_LOG_TEMP_FILE)) {
        Ok(()) => crate::logging::info(
            "chat_log_orphaned_temp_removed",
            serde_json::json!({ "storage_kind": "chat_log" }),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => crate::logging::warn(
            "chat_log_orphaned_temp_remove_failed",
            serde_json::json!({
                "storage_kind": "chat_log",
                "error_kind": format!("{:?}", error.kind()),
            }),
        ),
    }
}

/// Atomically publishes `snapshot` as the whole log, covering frames through
/// `covers`. Used to create a Task's log, to compact it, and to replace a Chat.
/// The replacement is written and read back before the rename; a failure
/// before it leaves the previous log authoritative and removes the temp file.
/// Returns the new file's length.
pub(super) fn publish_snapshot(
    task_dir: &Path,
    covers: u64,
    snapshot: &ChatSnapshotFile,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<u64, RuntimeError> {
    let target = path(task_dir);
    let temporary = task_dir.join(CHAT_LOG_TEMP_FILE);
    let result = (|| {
        let len = write_log(&temporary, covers, snapshot, kind, faults)?;
        faults.check(JournalKind::Compaction, FaultPoint::CompactionValidate)?;
        let verified = replay_file(&temporary, ReplayMode::read_only())?;
        if verified.covers != covers
            || !verified.frames.is_empty()
            || serde_json::to_value(&verified.snapshot)? != serde_json::to_value(snapshot)?
        {
            return Err(RuntimeError::Storage(
                "Chat log validation failed".to_string(),
            ));
        }
        faults.check(JournalKind::Compaction, FaultPoint::CompactionPublish)?;
        frame::durable_replace(&temporary, &target)?;
        frame::sync_directory(
            task_dir,
            JournalKind::Compaction,
            FaultPoint::CompactionPublishParentSync,
            faults,
        )?;
        Ok(len)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_log(
    temporary: &Path,
    covers: u64,
    snapshot: &ChatSnapshotFile,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<u64, RuntimeError> {
    let parent = temporary
        .parent()
        .ok_or_else(|| RuntimeError::Storage("Chat log path has no parent".to_string()))?;
    frame::create_directory_durably(parent, kind, faults)?;
    faults.check(kind, FaultPoint::CreateOpen)?;
    // A temp file an earlier attempt left is disposable: nothing references it.
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(temporary)
        .map_err(|error| frame::io_error("create_open", error))?;
    faults.check(kind, FaultPoint::CreateHeaderWrite)?;
    let mut header = Vec::with_capacity(LOG_HEADER_LEN);
    header.extend_from_slice(LOG_MAGIC);
    header.extend_from_slice(&LOG_FORMAT_VERSION.to_le_bytes());
    header.extend_from_slice(&covers.to_le_bytes());
    file.write_all(&header)
        .map_err(|error| frame::io_error("create_header_write", error))?;
    frame::write_frame(&mut file, snapshot, kind, faults)?;
    faults.check(kind, FaultPoint::FileSync)?;
    faults.record_sync();
    file.sync_all()
        .map_err(|error| frame::io_error("create_sync", error))?;
    let len = file
        .metadata()
        .map_err(|error| frame::io_error("create_metadata", error))?
        .len();
    frame::sync_directory(parent, kind, FaultPoint::ParentSync, faults)?;
    Ok(len)
}

/// Appends and confirms one committed transaction's frame.
pub(super) fn append(
    task_dir: &Path,
    frame_record: &ChatFrame,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    frame::append_with_faults(&path(task_dir), frame_record, kind, faults)
}

/// A decoded log.
pub(super) struct ReplayedLog {
    /// The last frame number the snapshot covers.
    pub(super) covers: u64,
    pub(super) snapshot: ChatSnapshotFile,
    pub(super) frames: Vec<ChatFrame>,
    /// Number of the last complete frame, or `covers` when there is none.
    pub(super) last_frame: u64,
}

#[derive(Clone, Copy)]
struct ReplayMode {
    /// Truncate an incomplete final frame.
    repair_tail: bool,
    /// Ignore frames numbered above this checkpoint.
    through: Option<u64>,
}

impl ReplayMode {
    fn read_only() -> Self {
        Self {
            repair_tail: false,
            through: None,
        }
    }
}

/// Replays the log for its owner: an incomplete final frame is truncated and
/// every complete frame is returned, including ones a crash left past the
/// `task.json` checkpoint, so the caller can roll them forward.
pub(super) fn replay_for_owner(task_dir: &Path) -> Result<ReplayedLog, RuntimeError> {
    replay_file(
        &path(task_dir),
        ReplayMode {
            repair_tail: true,
            through: None,
        },
    )
}

fn replay_file(log: &Path, mode: ReplayMode) -> Result<ReplayedLog, RuntimeError> {
    let mut file = File::open(log).map_err(|error| frame::io_error("replay_open", error))?;
    let file_len = file
        .metadata()
        .map_err(|error| frame::io_error("replay_metadata", error))?
        .len();
    let covers = read_header(&mut file)?;
    let mut cursor = LOG_HEADER_LEN as u64;
    let snapshot_payload =
        frame::read_payload(&mut file, file_len - cursor, covers)?.ok_or_else(|| {
            RuntimeError::Storage("Chat log snapshot record is incomplete".to_string())
        })?;
    cursor += frame::framed_len(snapshot_payload.len());
    let snapshot = decode_snapshot(&snapshot_payload)?;

    let mut frames = Vec::new();
    let mut last_frame = covers;
    while mode.through.is_none_or(|through| last_frame < through) {
        let expected = last_frame + 1;
        let Some(payload) = frame::read_payload(&mut file, file_len - cursor, expected)? else {
            break;
        };
        let record = ChatFrame::decode(&payload)?;
        if record.format_version() != frame::FORMAT_VERSION {
            return Err(RuntimeError::Storage(format!(
                "Unsupported journal frame version {}",
                record.format_version()
            )));
        }
        if record.sequence() != expected {
            return Err(RuntimeError::Storage(format!(
                "Chat log sequence gap: expected {expected}, found {}",
                record.sequence()
            )));
        }
        cursor += frame::framed_len(payload.len());
        last_frame = expected;
        frames.push(record);
    }
    if mode.repair_tail {
        frame::discard_incomplete_tail(log, cursor, file_len)?;
    }
    Ok(ReplayedLog {
        covers,
        snapshot,
        frames,
        last_frame,
    })
}

fn read_header(file: &mut File) -> Result<u64, RuntimeError> {
    let mut header = [0_u8; LOG_HEADER_LEN];
    file.read_exact(&mut header)
        .map_err(|_| RuntimeError::Storage("Invalid Chat log header".to_string()))?;
    if &header[..LOG_MAGIC.len()] != LOG_MAGIC {
        return Err(RuntimeError::Storage("Invalid Chat log header".to_string()));
    }
    let version = u16::from_le_bytes([header[8], header[9]]);
    if version != LOG_FORMAT_VERSION {
        return Err(RuntimeError::Storage(format!(
            "Unsupported Chat log version {version}"
        )));
    }
    Ok(u64::from_le_bytes(
        header[10..LOG_HEADER_LEN]
            .try_into()
            .expect("covered frame slice"),
    ))
}

fn decode_snapshot(payload: &[u8]) -> Result<ChatSnapshotFile, RuntimeError> {
    let snapshot: ChatSnapshotFile = serde_json::from_slice(payload)?;
    if snapshot.schema_version != super::split::CHAT_SCHEMA_VERSION {
        return Err(RuntimeError::Storage(format!(
            "Unsupported Chat snapshot schema version {}",
            snapshot.schema_version
        )));
    }
    Ok(snapshot)
}

/// What a log looks like without decoding or verifying any record.
pub(super) struct LogShape {
    pub(super) covers: u64,
    /// Number of the last complete frame.
    pub(super) last_frame: u64,
    /// False when bytes follow the last complete frame.
    pub(super) clean_tail: bool,
    /// Bytes of the snapshot record.
    pub(super) snapshot_bytes: u64,
    /// Bytes of the frames after the snapshot record.
    pub(super) delta_bytes: u64,
}

/// Walks the length prefixes only. Startup compares the result with the
/// `task.json` checkpoint for every Task, so the cost is one small read per
/// frame since the last compaction instead of a decode of the Chat.
pub(super) fn inspect(task_dir: &Path) -> Result<LogShape, RuntimeError> {
    let mut file =
        File::open(path(task_dir)).map_err(|error| frame::io_error("inspect_open", error))?;
    let file_len = file
        .metadata()
        .map_err(|error| frame::io_error("inspect_metadata", error))?
        .len();
    let covers = read_header(&mut file)?;
    let mut cursor = LOG_HEADER_LEN as u64;
    let snapshot_bytes = frame::skip_frame(&mut file, file_len - cursor)?.ok_or_else(|| {
        RuntimeError::Storage("Chat log snapshot record is incomplete".to_string())
    })?;
    cursor += snapshot_bytes;
    let mut last_frame = covers;
    while let Some(frame_bytes) = frame::skip_frame(&mut file, file_len - cursor)? {
        cursor += frame_bytes;
        last_frame += 1;
    }
    Ok(LogShape {
        covers,
        last_frame,
        clean_tail: cursor == file_len,
        snapshot_bytes,
        delta_bytes: cursor - LOG_HEADER_LEN as u64 - snapshot_bytes,
    })
}

/// The operations committed after a client's known Chat version.
#[allow(dead_code)] // Versions are asserted by tests; callers read them from the projection.
#[derive(Debug)]
pub(crate) enum ChatChangesSince {
    /// The known version is the latest.
    Current { version: u64 },
    /// The committed frames after the known version, oldest first.
    Deltas {
        version: u64,
        frames: Vec<ChatFrameChanges>,
    },
    /// The known version predates the log's snapshot or a replacement frame,
    /// so the changes cannot be replayed; the caller sends the whole Chat.
    TooOld { version: u64 },
    /// The known version is newer than anything committed.
    Ahead { version: u64 },
}

/// One committed transaction's Chat operations.
#[derive(Debug)]
#[allow(dead_code)] // The frame number is asserted by tests; callers read the projection version.
pub(crate) struct ChatFrameChanges {
    /// The Chat version after this frame.
    pub(crate) version: u64,
    pub(crate) operations: Vec<TaskOperation>,
}

/// Reads the frames after `known` that `task.json` has committed (`through`).
/// Never modifies the file, so it may run beside the storage worker: bytes
/// past `through` or an incomplete final frame are simply not read.
pub(super) fn changes_since(
    task_dir: &Path,
    known: u64,
    through: u64,
) -> Result<ChatChangesSince, RuntimeError> {
    if known > through {
        return Ok(ChatChangesSince::Ahead { version: through });
    }
    if known == through {
        return Ok(ChatChangesSince::Current { version: through });
    }
    let mut file =
        File::open(path(task_dir)).map_err(|error| frame::io_error("changes_open", error))?;
    let file_len = file
        .metadata()
        .map_err(|error| frame::io_error("changes_metadata", error))?
        .len();
    let covers = read_header(&mut file)?;
    if known < covers {
        return Ok(ChatChangesSince::TooOld { version: through });
    }
    let mut cursor = LOG_HEADER_LEN as u64;
    cursor += frame::skip_frame(&mut file, file_len - cursor)?.ok_or_else(|| {
        RuntimeError::Storage("Chat log snapshot record is incomplete".to_string())
    })?;
    for _ in covers..known {
        cursor += frame::skip_frame(&mut file, file_len - cursor)?.ok_or_else(|| {
            RuntimeError::Storage("Chat log is behind its committed checkpoint".to_string())
        })?;
    }
    let mut frames = Vec::new();
    let mut version = known;
    while version < through {
        let expected = version + 1;
        let payload =
            frame::read_payload(&mut file, file_len - cursor, expected)?.ok_or_else(|| {
                RuntimeError::Storage("Chat log is behind its committed checkpoint".to_string())
            })?;
        let record = ChatFrame::decode(&payload)?;
        if record.sequence() != expected {
            return Err(RuntimeError::Storage(format!(
                "Chat log sequence gap: expected {expected}, found {}",
                record.sequence()
            )));
        }
        cursor += frame::framed_len(payload.len());
        version = expected;
        if record.journal.operations.iter().any(is_replacement) {
            // A replacement is not a delta: a client older than it must reload.
            return Ok(ChatChangesSince::TooOld { version: through });
        }
        frames.push(ChatFrameChanges {
            version,
            operations: record.journal.operations,
        });
    }
    Ok(ChatChangesSince::Deltas {
        version: through,
        frames,
    })
}

/// Operations that rewrite the whole Chat instead of changing part of it.
fn is_replacement(operation: &TaskOperation) -> bool {
    matches!(
        operation,
        TaskOperation::ReplaceMessages { .. } | TaskOperation::ReplaceProjection { .. }
    )
}

/// The highest versions one log records.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct LogVersions {
    pub(super) file_format: u16,
    pub(super) snapshot_schema: Option<u16>,
    pub(super) frame_format: u16,
    pub(super) schema: u16,
}

/// The versions a log records, read without verifying or repairing it, so an
/// App Server can tell a newer writer's log from a damaged one (ADR-0078).
pub(super) fn recorded_versions(log: &Path) -> Result<LogVersions, RuntimeError> {
    #[derive(Deserialize)]
    struct RecordVersions {
        #[serde(default)]
        format_version: u16,
        #[serde(default, alias = "schemaVersion")]
        schema_version: u16,
    }
    let bytes = fs::read(log).map_err(|error| frame::io_error("versions_read", error))?;
    if bytes.len() < LOG_HEADER_LEN || &bytes[..LOG_MAGIC.len()] != LOG_MAGIC {
        return Err(RuntimeError::Storage("Invalid Chat log header".to_string()));
    }
    let mut versions = LogVersions {
        file_format: u16::from_le_bytes([bytes[8], bytes[9]]),
        ..LogVersions::default()
    };
    if versions.file_format != LOG_FORMAT_VERSION {
        // Another file format may frame its payloads differently.
        return Ok(versions);
    }
    // The snapshot record has no frame format; its schema is the first record's.
    let mut first = true;
    let mut cursor = LOG_HEADER_LEN;
    while let Some(length) = bytes.get(cursor..cursor + 8) {
        let length = u64::from_le_bytes(length.try_into().expect("frame length slice"));
        let start = cursor + 8;
        let Some(payload) = usize::try_from(length)
            .ok()
            .and_then(|length| start.checked_add(length))
            .and_then(|end| bytes.get(start..end))
        else {
            break;
        };
        if let Ok(record) = serde_json::from_slice::<RecordVersions>(payload) {
            if first {
                versions.snapshot_schema = Some(record.schema_version);
            } else {
                versions.frame_format = versions.frame_format.max(record.format_version);
                versions.schema = versions.schema.max(record.schema_version);
            }
        }
        first = false;
        cursor = start + payload.len() + 4;
    }
    Ok(versions)
}
