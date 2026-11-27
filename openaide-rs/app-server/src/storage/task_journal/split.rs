//! `task.json` and the orchestration of one Task's durable files: the atomically
//! replaced metadata record, the `chat.log` Chat (see `chat_log.rs`), recovery,
//! compaction and the conversion of older layouts.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::errors::RuntimeError;
use crate::storage::atomic::{replace_file as durable_replace, sync_directory};
use crate::storage::records::TaskRecord;

use super::chat_log::{self, ChatFrame, ChatSnapshotFile, TaskMetadataPatch};
use super::frame::{FaultInjector, FaultPoint, JournalKind};
use super::generation_layout::{self, GenerationMetadata};
use super::model::{JournalFrame, TaskOperation, TaskProjection};
use super::store::RecoveredTask;

pub(super) const TASK_FILE: &str = "task.json";
/// Version 2 names a `chat.log`; version 1 named Chat generations
/// (`generation_layout.rs`) and is converted when its Task hydrates.
const TASK_SCHEMA_VERSION: u16 = 2;
/// Version 3 stores Chat images as references (ADR-0075). A version 2 Chat
/// reads unchanged; hydration moves any inline images out and compacts it.
pub(super) const CHAT_SCHEMA_VERSION: u16 = 3;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DurableTaskFile {
    schema_version: u16,
    storage_sequence: u64,
    /// Number of the last committed Chat frame: the startup checkpoint and the
    /// Task's Chat version. A mismatch with `chat.log` only triggers a scan of
    /// that log, never wrong data.
    chat_frame: u64,
    task: TaskRecord,
}

pub(super) struct TaskMetadata {
    pub(super) task: TaskRecord,
    pub(super) storage_sequence: u64,
    pub(super) chat_frame: u64,
}

/// `task.json` in either layout. Only a current Task is written.
pub(super) enum TaskFile {
    Current(TaskMetadata),
    Generations(GenerationMetadata),
}

impl TaskFile {
    /// The last committed Chat frame number in either layout.
    pub(super) fn chat_frame(&self) -> u64 {
        match self {
            Self::Current(metadata) => metadata.chat_frame,
            Self::Generations(metadata) => metadata.chat_sequence,
        }
    }

    pub(super) fn task(&self) -> &TaskRecord {
        match self {
            Self::Current(metadata) => &metadata.task,
            Self::Generations(metadata) => &metadata.task,
        }
    }

    pub(super) fn storage_sequence(&self) -> u64 {
        match self {
            Self::Current(metadata) => metadata.storage_sequence,
            Self::Generations(metadata) => metadata.storage_sequence,
        }
    }
}

pub(super) fn read_task_file(task_dir: &Path) -> Result<Option<TaskFile>, RuntimeError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope {
        schema_version: u16,
    }
    let bytes = match fs::read(task_dir.join(TASK_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    // The envelope is read first, so a newer release's file is refused before
    // any field of it is interpreted.
    let envelope: Envelope = serde_json::from_slice(&bytes)?;
    let mut file = match envelope.schema_version {
        TASK_SCHEMA_VERSION => {
            let file: DurableTaskFile = serde_json::from_slice(&bytes)?;
            TaskFile::Current(TaskMetadata {
                task: file.task,
                storage_sequence: file.storage_sequence,
                chat_frame: file.chat_frame,
            })
        }
        generation_layout::TASK_SCHEMA_VERSION => {
            TaskFile::Generations(generation_layout::parse(&bytes)?)
        }
        version => {
            return Err(RuntimeError::Storage(format!(
                "Unsupported Task metadata schema version {version}"
            )))
        }
    };
    // Older releases persisted option catalogs. Never hydrate those as live controls.
    let task = match &mut file {
        TaskFile::Current(metadata) => &mut metadata.task,
        TaskFile::Generations(metadata) => &mut metadata.task,
    };
    task.config_options_catalog = None;
    task.config_mutation.preferences = None;
    Ok(Some(file))
}

/// Metadata of a Task whose Chat is in `chat.log`. A Task still in the
/// generation layout is refused: its hydration converts it first.
pub(super) fn load_task(task_dir: &Path) -> Result<Option<TaskMetadata>, RuntimeError> {
    match read_task_file(task_dir)? {
        None => Ok(None),
        Some(TaskFile::Current(metadata)) => Ok(Some(metadata)),
        Some(TaskFile::Generations(_)) => Err(RuntimeError::Storage(
            "Task Chat is not converted to chat.log".to_string(),
        )),
    }
}

pub(super) fn exists(task_dir: &Path) -> bool {
    match read_task_file(task_dir) {
        Ok(Some(TaskFile::Current(_))) => chat_log::exists(task_dir),
        Ok(Some(TaskFile::Generations(metadata))) => generation_layout::exists(task_dir, &metadata),
        _ => false,
    }
}

/// Startup must recover Send promotion and queue consumption before a metadata
/// reader can dispose a Prepared Task or offer accepted work for delivery again.
/// The check is the checkpoint against the log's frame count; only a mismatch
/// scans that one Task. A Task still in the generation layout keeps its byte
/// checkpoint check and converts when it has an uncommitted frame.
pub(super) fn load_catalog_task(task_dir: &Path) -> Result<Option<TaskMetadata>, RuntimeError> {
    let metadata = match read_task_file(task_dir)? {
        None => return Ok(None),
        Some(TaskFile::Generations(old)) => {
            if !generation_layout::has_uncommitted_bytes(task_dir, &old)? {
                return Ok(Some(TaskMetadata {
                    task: old.task,
                    storage_sequence: old.storage_sequence,
                    chat_frame: old.chat_sequence,
                }));
            }
            return scan_task(
                task_dir,
                &old.task.task_id,
                old.chat_sequence,
                "generation_tail",
            );
        }
        Some(TaskFile::Current(metadata)) => metadata,
    };
    chat_log::remove_orphaned_temp(task_dir);
    let reason = match chat_log::inspect(task_dir) {
        Ok(shape) if shape.clean_tail && shape.last_frame == metadata.chat_frame => {
            return Ok(Some(metadata))
        }
        Ok(shape) if shape.clean_tail => "frame_mismatch",
        Ok(_) => "incomplete_tail",
        Err(error) => {
            // A log that cannot even be walked is left to its Task's hydration,
            // which fails closed (or sets a newer Chat aside, ADR-0078), so one
            // damaged Chat never hides its Task from Navigation.
            crate::logging::warn(
                "chat_log_inspect_failed",
                serde_json::json!({
                    "operation": "chat_log_startup_scan",
                    "task_id": metadata.task.task_id,
                    "error_kind": error.reason(),
                }),
            );
            return Ok(Some(metadata));
        }
    };
    scan_task(
        task_dir,
        &metadata.task.task_id,
        metadata.chat_frame,
        reason,
    )
}

/// The one-time scan of a Task whose checkpoint does not match its Chat.
fn scan_task(
    task_dir: &Path,
    task_id: &str,
    checkpoint: u64,
    reason: &str,
) -> Result<Option<TaskMetadata>, RuntimeError> {
    let started = Instant::now();
    crate::logging::info(
        "task_metadata_recovery_started",
        serde_json::json!({
            "operation": "chat_log_startup_scan",
            "task_id": task_id,
            "chat_frame": checkpoint,
            "reason": reason,
        }),
    );
    let recovery = load_projection(task_dir);
    // A newer App Server's Chat is rebuilt when its Task loads (ADR-0078),
    // so it stays listed instead of becoming unavailable at startup.
    let newer = recovery.is_err() && super::newer_chat::detect(task_dir).is_some();
    crate::logging::info(
        "task_metadata_recovery_finished",
        serde_json::json!({
            "operation": "chat_log_startup_scan",
            "task_id": task_id,
            "reason": reason,
            "outcome": match (&recovery, newer) {
                (Ok(_), _) => "recovered",
                (Err(_), true) => "newer_chat",
                (Err(_), false) => "unavailable",
            },
            "error_kind": recovery.as_ref().err().map(RuntimeError::reason),
            "duration_ms": started.elapsed().as_millis(),
        }),
    );
    if newer {
        return Ok(match read_task_file(task_dir)? {
            Some(TaskFile::Current(metadata)) => Some(metadata),
            Some(TaskFile::Generations(old)) => Some(TaskMetadata {
                task: old.task,
                storage_sequence: old.storage_sequence,
                chat_frame: old.chat_sequence,
            }),
            None => None,
        });
    }
    recovery?;
    load_task(task_dir)
}

/// Hydrates the Task: the log's snapshot plus its frames, with a complete
/// frame past the checkpoint rolled forward, an incomplete final frame
/// discarded, and frames the checkpoint covers unable to override `task.json`.
/// A Task still in the generation layout is converted first.
pub(super) fn load_projection(
    task_dir: &Path,
) -> Result<Option<(TaskProjection, u64)>, RuntimeError> {
    let metadata = match read_task_file(task_dir)? {
        None => return Ok(None),
        Some(TaskFile::Generations(_)) => {
            generation_layout::migrate(task_dir, &FaultInjector::disabled())?;
            match load_task(task_dir)? {
                Some(metadata) => metadata,
                None => return Ok(None),
            }
        }
        Some(TaskFile::Current(metadata)) => metadata,
    };
    chat_log::remove_orphaned_temp(task_dir);
    if !task_dir.join(super::newer_chat::PENDING_DIR).exists() {
        // Files of the generation layout a conversion could not remove. A
        // pending set-aside still owns them.
        if let Err(error) = generation_layout::remove_files(task_dir) {
            crate::logging::warn(
                "chat_generation_cleanup_failed",
                serde_json::json!({
                    "task_id": metadata.task.task_id,
                    "error_kind": error.reason(),
                }),
            );
        }
    }
    let replayed = chat_log::replay_for_owner(task_dir)?;
    if replayed.last_frame < metadata.chat_frame {
        return Err(RuntimeError::Storage(
            "Chat log is behind its committed Task metadata".to_string(),
        ));
    }
    let projection = TaskProjection {
        task: metadata.task.clone(),
        messages: replayed.snapshot.messages,
        message_meta: replayed.snapshot.message_meta,
        artifact_heads: replayed.snapshot.artifact_heads,
        // TODO(state-sync): set the real version here instead of relying on hydration to overwrite it.
        chat_version: 0,
    };
    let task_id = metadata.task.task_id.clone();
    let replayed_frames = !replayed.frames.is_empty();
    let mut state = std::collections::HashMap::from([(
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
        let is_checkpoint = frame.sequence == metadata.chat_frame;
        let needs_recovery = frame.sequence > metadata.chat_frame;
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
        projection: replayed_projection,
        ..
    } = state.remove(&task_id).expect("split replay retains Task")
    else {
        unreachable!("split replay cannot quarantine Task")
    };
    let mut projection = *replayed_projection;
    if replayed_frames {
        projection.task.message_history_version = projection.message_meta.version;
    }
    if replayed.last_frame > metadata.chat_frame {
        let repaired_storage_sequence = metadata
            .storage_sequence
            .saturating_add(replayed.last_frame - metadata.chat_frame);
        publish_task(
            task_dir,
            &projection.task,
            repaired_storage_sequence,
            replayed.last_frame,
            &FaultInjector::disabled(),
        )?;
        return Ok(Some((projection, repaired_storage_sequence)));
    }
    Ok(Some((projection, metadata.storage_sequence)))
}

fn snapshot_of(projection: &TaskProjection) -> ChatSnapshotFile {
    ChatSnapshotFile {
        schema_version: CHAT_SCHEMA_VERSION,
        messages: projection.messages.clone(),
        message_meta: projection.message_meta.clone(),
        artifact_heads: projection.artifact_heads.clone(),
    }
}

/// Publishes the initial Chat log before metadata becomes discoverable.
pub(super) fn publish_initial(
    task_dir: &Path,
    projection: &TaskProjection,
    storage_sequence: u64,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    fs::create_dir_all(task_dir)?;
    chat_log::publish_snapshot(
        task_dir,
        0,
        &snapshot_of(projection),
        JournalKind::Task,
        faults,
    )?;
    publish_task(task_dir, &projection.task, storage_sequence, 0, faults)
}

/// Appends only Chat-affecting operations, then publishes compact Task
/// metadata. Returns the Chat version after the commit.
pub(super) fn append(
    task_dir: &Path,
    projection: &TaskProjection,
    operations: &[TaskOperation],
    storage_sequence: u64,
    journal_kind: JournalKind,
    faults: &FaultInjector,
) -> Result<u64, RuntimeError> {
    let metadata = load_task(task_dir)?
        .ok_or_else(|| RuntimeError::Storage("Split Task metadata is missing".to_string()))?;
    let chat_operations = durable_chat_operations(operations);
    let next_chat_frame = if chat_operations.is_empty() {
        metadata.chat_frame
    } else {
        let next = metadata
            .chat_frame
            .checked_add(1)
            .ok_or_else(|| RuntimeError::Storage("Chat frame number overflow".to_string()))?;
        let frame = ChatFrame {
            journal: JournalFrame {
                format_version: 1,
                schema_version: CHAT_SCHEMA_VERSION,
                sequence: next,
                operations: chat_operations,
            },
            task_patch: TaskMetadataPatch::between(&metadata.task, &projection.task)?,
        };
        chat_log::append(task_dir, &frame, journal_kind, faults)?;
        next
    };
    publish_task(
        task_dir,
        &projection.task,
        storage_sequence,
        next_chat_frame,
        faults,
    )?;
    Ok(next_chat_frame)
}

/// Rewrites the log as one snapshot record covering every committed frame and
/// renames it over the old log. The rename is the commit: a crash before it
/// leaves an orphaned temp file the next load removes, and one after it keeps
/// numbering where it was, so `task.json` needs no change.
pub(super) fn compact(
    task_dir: &Path,
    projection: &TaskProjection,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let started = Instant::now();
    let metadata = load_task(task_dir)?
        .ok_or_else(|| RuntimeError::Storage("Split Task metadata is missing".to_string()))?;
    let before = chat_log::inspect(task_dir).ok();
    crate::logging::info(
        "chat_log_compaction_started",
        serde_json::json!({
            "operation": "chat_log_compaction",
            "task_id": metadata.task.task_id,
            "chat_frame": metadata.chat_frame,
            "frames": before.as_ref().map(|shape| shape.last_frame - shape.covers),
            "old_bytes": before.as_ref().map(|shape| shape.delta_bytes + shape.snapshot_bytes),
        }),
    );
    let result = chat_log::publish_snapshot(
        task_dir,
        metadata.chat_frame,
        &snapshot_of(projection),
        JournalKind::Compaction,
        faults,
    );
    crate::logging::info(
        "chat_log_compaction_finished",
        serde_json::json!({
            "operation": "chat_log_compaction",
            "task_id": metadata.task.task_id,
            "outcome": if result.is_ok() { "compacted" } else { "failed" },
            "error_kind": result.as_ref().err().map(RuntimeError::reason),
            "new_bytes": result.as_ref().ok(),
            "duration_ms": started.elapsed().as_millis(),
        }),
    );
    result?;
    super::images::sweep_unreferenced(task_dir, projection);
    Ok(())
}

/// Replaces the whole Chat with `projection` as one snapshot covering the next
/// frame number, so a client holding any earlier version sees the Chat as
/// replaced. `task.json` names it, which is the commit. The caller keeps the
/// replaced log if it needs it.
pub(super) fn publish_replacement(
    task_dir: &Path,
    projection: &TaskProjection,
    storage_sequence: u64,
    covers: u64,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    chat_log::publish_snapshot(
        task_dir,
        covers,
        &snapshot_of(projection),
        JournalKind::Compaction,
        faults,
    )?;
    publish_task(task_dir, &projection.task, storage_sequence, covers, faults)
}

pub(super) fn compaction_is_worthwhile(task_dir: &Path) -> Result<bool, RuntimeError> {
    let shape = chat_log::inspect(task_dir)?;
    if shape.last_frame - shape.covers >= 128 {
        return Ok(true);
    }
    Ok(shape.delta_bytes >= shape.snapshot_bytes && shape.delta_bytes >= 1024 * 1024)
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

pub(super) fn publish_task(
    task_dir: &Path,
    task: &TaskRecord,
    storage_sequence: u64,
    chat_frame: u64,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let mut task = task.clone();
    task.clear_process_local_agent_state();
    durable_replace_json(
        &task_dir.join(TASK_FILE),
        &DurableTaskFile {
            schema_version: TASK_SCHEMA_VERSION,
            storage_sequence,
            chat_frame,
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
        faults.check(kind, FaultPoint::DirectoryParentSync)?;
        faults.check(kind, FaultPoint::CreateOpen)?;
        faults.check(kind, FaultPoint::AppendOpen)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        faults.check(kind, FaultPoint::CreateHeaderWrite)?;
        faults.check(kind, FaultPoint::FrameLengthWrite)?;
        faults.check(kind, FaultPoint::FramePayloadWrite)?;
        file.write_all(&bytes)?;
        faults.check(kind, FaultPoint::FrameChecksumWrite)?;
        faults.check(kind, FaultPoint::FileSync)?;
        faults.record_sync();
        file.sync_all()?;
        durable_replace(&temporary, path)?;
        faults.check(kind, FaultPoint::ParentSync)?;
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
