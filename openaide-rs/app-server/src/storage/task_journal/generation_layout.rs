//! The Chat layout ADR-0029 introduced and ADR-0086 replaced: `task.json`
//! (schema 1) names a `chat.snapshot.<generation>` and a
//! `chat.journal.<generation>` whose frame numbers restart at 1 per
//! generation. This module only reads that layout and converts it to
//! `chat.log`; nothing writes it any more.
//!
//! The conversion runs lazily at the Task's normal hydration boundary. It
//! writes and validates the whole `chat.log`, publishes it with the schema 2
//! `task.json` (the commit point), and only then removes the old files. A
//! failure before the commit leaves the old bytes authoritative and the next
//! hydration retries.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::Deserialize;
use serde_json::Value;

use crate::protocol::errors::RuntimeError;
use crate::storage::atomic::sync_directory;
use crate::storage::records::TaskRecord;

use super::chat_log::{self, ChatFrame, ChatSnapshotFile};
use super::frame::{self, FaultInjector, JournalKind};
use super::model::TaskProjection;
use super::split::{self, CHAT_SCHEMA_VERSION};
use super::store::RecoveredTask;

pub(super) const CHAT_SNAPSHOT_FILE: &str = "chat.snapshot";
pub(super) const CHAT_JOURNAL_FILE: &str = "chat.journal";
/// The `task.json` schema version of this layout.
pub(super) const TASK_SCHEMA_VERSION: u16 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerationTaskFile {
    storage_sequence: u64,
    chat_sequence: u64,
    chat_snapshot: String,
    chat_journal: String,
    /// Optional for stores written before interrupted Chat commits were
    /// detected at Navigation startup.
    #[serde(default)]
    chat_journal_bytes: Option<u64>,
    task: TaskRecord,
}

/// What `task.json` says about a Task whose Chat is still in generations.
pub(super) struct GenerationMetadata {
    pub(super) task: TaskRecord,
    pub(super) storage_sequence: u64,
    /// Number of the last committed frame in the current generation.
    pub(super) chat_sequence: u64,
    pub(super) chat_snapshot: String,
    pub(super) chat_journal: String,
    chat_journal_bytes: Option<u64>,
}

pub(super) fn parse(bytes: &[u8]) -> Result<GenerationMetadata, RuntimeError> {
    let file: GenerationTaskFile = serde_json::from_slice(bytes)?;
    Ok(GenerationMetadata {
        task: file.task,
        storage_sequence: file.storage_sequence,
        chat_sequence: file.chat_sequence,
        chat_snapshot: file.chat_snapshot,
        chat_journal: file.chat_journal,
        chat_journal_bytes: file.chat_journal_bytes,
    })
}

pub(super) fn exists(task_dir: &Path, metadata: &GenerationMetadata) -> bool {
    task_dir.join(&metadata.chat_snapshot).is_file()
}

/// Whether the journal holds bytes past the committed length `task.json`
/// recorded: a Chat frame a crash left before the metadata replacement.
/// Startup must recover those before a reader can dispose a Prepared Task or
/// offer accepted work again. A file without the checkpoint stays lazy.
pub(super) fn has_uncommitted_bytes(
    task_dir: &Path,
    metadata: &GenerationMetadata,
) -> Result<bool, RuntimeError> {
    let Some(committed) = metadata.chat_journal_bytes else {
        return Ok(false);
    };
    let journal_bytes = match fs::metadata(task_dir.join(&metadata.chat_journal)) {
        Ok(found) => found.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    Ok(journal_bytes > committed)
}

/// A generation Task's Chat as hydration recovers it. Reads only.
struct LoadedGeneration {
    projection: TaskProjection,
    storage_sequence: u64,
    /// Number of the last complete frame in the generation.
    last_frame: u64,
    frame_count: usize,
}

/// Replays the current generation as the previous hydration did: a complete
/// frame past the checkpoint rolls forward, an incomplete final frame is
/// ignored, and frames the checkpoint covers cannot override `task.json`.
fn load_projection(
    task_dir: &Path,
    metadata: &GenerationMetadata,
) -> Result<LoadedGeneration, RuntimeError> {
    let (snapshot, _) = load_chat_snapshot(&task_dir.join(&metadata.chat_snapshot))?;
    let projection = TaskProjection {
        task: metadata.task.clone(),
        messages: snapshot.messages,
        message_meta: snapshot.message_meta,
        artifact_heads: snapshot.artifact_heads,
        chat_version: 0,
    };
    let journal = task_dir.join(&metadata.chat_journal);
    // A crash can interrupt creation before even the first frame header. No
    // committed reference exists, so such a generation holds no frames.
    let journal_is_empty = metadata.chat_sequence == 0
        && journal.is_file()
        && fs::metadata(&journal)?.len() < frame::FILE_HEADER_LEN as u64;
    if journal_is_empty || (!journal.is_file() && metadata.chat_sequence == 0) {
        return Ok(LoadedGeneration {
            projection,
            storage_sequence: metadata.storage_sequence,
            last_frame: 0,
            frame_count: 0,
        });
    }
    if !journal.is_file() {
        return Err(RuntimeError::Storage(
            "Committed Chat journal generation is missing".to_string(),
        ));
    }
    let replayed: frame::ReplayedFrames<ChatFrame> = frame::replay_read_only(&journal)?;
    let last_frame = replayed.frame_count as u64;
    if last_frame < metadata.chat_sequence {
        return Err(RuntimeError::Storage(
            "Chat journal is behind its committed Task metadata".to_string(),
        ));
    }
    let frame_count = replayed.frame_count;
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
            let RecoveredTask::Available { projection, .. } = state
                .get_mut(&task_id)
                .expect("generation replay retains Task")
            else {
                unreachable!("generation replay cannot quarantine Task")
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
    } = state
        .remove(&task_id)
        .expect("generation replay retains Task")
    else {
        unreachable!("generation replay cannot quarantine Task")
    };
    let mut projection = *replayed;
    if frame_count > 0 {
        projection.task.message_history_version = projection.message_meta.version;
    }
    let storage_sequence = metadata
        .storage_sequence
        .saturating_add(last_frame.saturating_sub(metadata.chat_sequence));
    Ok(LoadedGeneration {
        projection,
        storage_sequence,
        last_frame,
        frame_count,
    })
}

/// Converts a generation Task to `chat.log` and returns the storage sequence
/// the converted Task starts from. The frame numbers continue from the
/// generation's last frame, which the log's snapshot covers.
pub(super) fn migrate(task_dir: &Path, faults: &FaultInjector) -> Result<u64, RuntimeError> {
    let started = Instant::now();
    let metadata = match split::read_task_file(task_dir)? {
        Some(split::TaskFile::Generations(metadata)) => metadata,
        _ => {
            return Err(RuntimeError::Storage(
                "Task is not in the generation layout".to_string(),
            ))
        }
    };
    let task_id = metadata.task.task_id.clone();
    let old_bytes = layout_bytes(task_dir, &metadata);
    crate::logging::info(
        "chat_log_migration_started",
        serde_json::json!({
            "operation": "chat_log_migration",
            "task_id": task_id,
            "frames": metadata.chat_sequence,
            "old_bytes": old_bytes,
        }),
    );
    let result: Result<_, RuntimeError> = (|| {
        let loaded = load_projection(task_dir, &metadata)?;
        let projection = &loaded.projection;
        let snapshot = ChatSnapshotFile {
            schema_version: CHAT_SCHEMA_VERSION,
            messages: projection.messages.clone(),
            message_meta: projection.message_meta.clone(),
            artifact_heads: projection.artifact_heads.clone(),
        };
        // Written and read back before it can be referenced.
        let log_bytes = chat_log::publish_snapshot(
            task_dir,
            loaded.last_frame,
            &snapshot,
            JournalKind::Compaction,
            faults,
        )?;
        // The commit point: the schema 2 `task.json` names the converted Chat.
        split::publish_task(
            task_dir,
            &projection.task,
            loaded.storage_sequence,
            loaded.last_frame,
            faults,
        )?;
        Ok((loaded.storage_sequence, loaded.frame_count, log_bytes))
    })();
    match result {
        Ok((storage_sequence, replayed_frames, log_bytes)) => {
            // The converted Chat is authoritative. Cleanup is retried at the
            // Task's next hydration and must not fail this one.
            let cleanup = remove_files(task_dir);
            crate::logging::info(
                "chat_log_migration_finished",
                serde_json::json!({
                    "operation": "chat_log_migration",
                    "task_id": task_id,
                    "outcome": "migrated",
                    "frames": metadata.chat_sequence,
                    "replayed_frames": replayed_frames,
                    "old_bytes": old_bytes,
                    "new_bytes": log_bytes,
                    "cleanup": if cleanup.is_ok() { "removed" } else { "failed" },
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
            Ok(storage_sequence)
        }
        Err(error) => {
            crate::logging::warn(
                "chat_log_migration_finished",
                serde_json::json!({
                    "operation": "chat_log_migration",
                    "task_id": task_id,
                    "outcome": "failed",
                    "error_kind": error.reason(),
                    "old_bytes": old_bytes,
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
            Err(error)
        }
    }
}

fn layout_bytes(task_dir: &Path, metadata: &GenerationMetadata) -> u64 {
    [&metadata.chat_snapshot, &metadata.chat_journal]
        .into_iter()
        .filter_map(|name| fs::metadata(task_dir.join(name)).ok())
        .map(|found| found.len())
        .sum()
}

/// Removes every generation file in the Task directory: the current pair and
/// any pair an interrupted compaction left. Subagent histories keep their own
/// files below `subagents/`, which this never touches.
pub(super) fn remove_files(task_dir: &Path) -> Result<(), RuntimeError> {
    let mut removed = false;
    for entry in fs::read_dir(task_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if is_generation_file(name) && entry.file_type()?.is_file() {
            fs::remove_file(entry.path())?;
            removed = true;
        }
    }
    if removed {
        sync_directory(task_dir)?;
    }
    Ok(())
}

/// A name in the generation layout: the unsuffixed pair or one per generation.
pub(super) fn is_generation_file(name: &str) -> bool {
    [CHAT_SNAPSHOT_FILE, CHAT_JOURNAL_FILE].iter().any(|base| {
        name == *base
            || name
                .strip_prefix(base)
                .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// The schema version a Chat snapshot records, read without loading it.
pub(super) fn snapshot_schema_version(path: &Path) -> Option<u16> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SnapshotVersion {
        schema_version: u16,
    }
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice::<SnapshotVersion>(&bytes)
        .ok()
        .map(|snapshot| snapshot.schema_version)
}

/// Decodes a snapshot of any released schema into the current one.
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
        2 => {
            // Its images move at hydration, which publishes a version 3 log.
            value["schemaVersion"] = Value::from(CHAT_SCHEMA_VERSION);
            false
        }
        1 => {
            split::migrate_v1_tool_presentations(&mut value);
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
    Ok((snapshot, migrated))
}

#[cfg(test)]
pub(super) mod test_support {
    use std::path::{Path, PathBuf};

    use super::*;

    /// The files of a Task written in the generation layout.
    pub(in crate::storage::task_journal) struct GenerationFiles {
        pub(in crate::storage::task_journal) snapshot: PathBuf,
    }

    /// Rewrites a stored current Task into the layout releases before ADR-0086
    /// wrote: a `task.json` of schema 1 naming a snapshot and a journal whose
    /// frame numbers restart at 1. Tests edit the returned files to stage
    /// content that older releases wrote.
    pub(in crate::storage::task_journal) fn downgrade(task_dir: &Path) -> GenerationFiles {
        let Some(split::TaskFile::Current(metadata)) = split::read_task_file(task_dir).unwrap()
        else {
            panic!("downgrade needs a current Task");
        };
        let replayed = chat_log::replay_for_owner(task_dir).unwrap();
        let snapshot_name = "chat.snapshot.7f3a9c";
        let journal_name = "chat.journal.7f3a9c";
        fs::write(
            task_dir.join(snapshot_name),
            serde_json::to_vec(&replayed.snapshot).unwrap(),
        )
        .unwrap();
        let journal = task_dir.join(journal_name);
        for (index, mut record) in replayed.frames.into_iter().enumerate() {
            record.journal.sequence = index as u64 + 1;
            if index == 0 {
                frame::create(&journal, &record).unwrap();
            } else {
                frame::append(&journal, &record).unwrap();
            }
        }
        let journal_bytes = fs::metadata(&journal).map_or(0, |found| found.len());
        let committed = metadata.chat_frame - replayed.covers;
        fs::write(
            task_dir.join(split::TASK_FILE),
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": TASK_SCHEMA_VERSION,
                "storageSequence": metadata.storage_sequence,
                "chatSequence": committed,
                "chatSnapshot": snapshot_name,
                "chatJournal": journal_name,
                "chatJournalBytes": journal_bytes,
                "task": metadata.task,
            }))
            .unwrap(),
        )
        .unwrap();
        fs::remove_file(chat_log::path(task_dir)).unwrap();
        GenerationFiles {
            snapshot: task_dir.join(snapshot_name),
        }
    }

    /// The snapshot record of a Task's `chat.log` as JSON.
    pub(in crate::storage::task_journal) fn log_snapshot_json(task_dir: &Path) -> Value {
        let replayed = chat_log::replay_for_owner(task_dir).unwrap();
        serde_json::to_value(replayed.snapshot).unwrap()
    }
}

#[cfg(test)]
#[path = "generation_layout_tests.rs"]
mod tests;
