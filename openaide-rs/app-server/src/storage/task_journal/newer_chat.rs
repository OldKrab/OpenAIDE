//! A Task's Chat written by a newer App Server (ADR-0078).
//!
//! This App Server cannot read it, so it sets the Chat's files aside and starts
//! an empty Chat with a Native Session reload requirement. The next Task open
//! fills it through `session/load`. Durable Task Metadata stays in `task.json`.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::storage::atomic::{self, sync_directory};
use crate::storage::chat_images::IMAGES_DIR;
use crate::storage::records::MessageMeta;
use crate::storage::subagents::{self, SubagentState};

use super::artifact::ARTIFACTS_DIR;
use super::frame::{self, FaultInjector};
use super::model::TaskProjection;
use super::split;

/// The latest set-aside Chat, kept for manual recovery until a later set-aside
/// replaces it or the Task is deleted. Nothing reads it back.
const SET_ASIDE_DIR: &str = "set-aside";
/// A set-aside in progress; the next load of its Task settles it.
const PENDING_DIR: &str = "set-aside.pending";
/// Inside the pending directory, written before anything moves into it.
const RECORD_FILE: &str = "set-aside.json";

/// The newer Chat part that this App Server cannot read.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NewerChat {
    part: String,
    version: u16,
    known: u16,
}

/// Why a set-aside started and which generation it replaces. The next load
/// compares the generation with `task.json` to tell an uncommitted set-aside,
/// which it resumes, from a committed one, which it only finishes. It stays
/// in `set-aside/` to explain the set-aside to a manual recovery.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetAsideRecord {
    #[serde(flatten)]
    newer: NewerChat,
    chat_snapshot: String,
    chat_journal: String,
}

/// Recognizes a Chat that failed to load because a newer App Server wrote it.
/// Reads only version fields and repairs nothing; `None` keeps the load's own
/// error, so a damaged Chat still fails closed.
pub(super) fn detect(task_dir: &Path) -> Option<NewerChat> {
    let metadata = split::load_task(task_dir).ok()??;
    let snapshot = split::snapshot_schema_version(&task_dir.join(&metadata.chat_snapshot));
    let journal = frame::recorded_versions(&task_dir.join(&metadata.chat_journal)).ok();
    [
        ("chat_snapshot", snapshot, split::CHAT_SCHEMA_VERSION),
        (
            "chat_journal_format",
            journal.as_ref().map(|recorded| recorded.file_format),
            frame::FORMAT_VERSION,
        ),
        (
            "chat_frame_format",
            journal.as_ref().map(|recorded| recorded.frame_format),
            frame::FORMAT_VERSION,
        ),
        (
            "chat_journal_schema",
            journal.as_ref().map(|recorded| recorded.schema),
            split::CHAT_SCHEMA_VERSION,
        ),
    ]
    .into_iter()
    .find_map(|(part, version, known)| {
        version
            .filter(|version| *version > known)
            .map(|version| NewerChat {
                part: part.to_string(),
                version,
                known,
            })
    })
    .or_else(|| detect_subagents(task_dir))
}

/// A newer Subagent catalog makes the whole Chat newer: the Native Session
/// replay restores the main Chat and its Subagents together.
pub(super) fn detect_subagents(task_dir: &Path) -> Option<NewerChat> {
    subagents::newer_catalog_version(task_dir).map(|version| NewerChat {
        part: "subagent_catalog".to_string(),
        version,
        known: subagents::CATALOG_SCHEMA_VERSION,
    })
}

/// Sets the Chat aside and publishes an empty one that requires a Native
/// Session reload. Returns the published projection and its storage sequence.
pub(super) fn rebuild(
    task_dir: &Path,
    newer: &NewerChat,
    subagent_state: &SubagentState,
) -> Result<(TaskProjection, u64), RuntimeError> {
    let started = Instant::now();
    let metadata = split::load_task(task_dir)?
        .ok_or_else(|| RuntimeError::Storage("Split Task metadata is missing".to_string()))?;
    let task_id = metadata.task.task_id.clone();
    crate::logging::info(
        "newer_chat_set_aside_started",
        serde_json::json!({
            "task_id": task_id,
            "part": newer.part,
            "version": newer.version,
            "known_version": newer.known,
        }),
    );
    let result: Result<_, RuntimeError> = (|| {
        let pending = begin(task_dir, &metadata, newer)?;
        // Images, Tool artifacts, and Subagents leave before the commit, and the
        // Chat files stay referenced until it, so a rebuild interrupted before
        // its commit resumes here with the same pending directory.
        rename_if_present(&task_dir.join(IMAGES_DIR), &pending.join(IMAGES_DIR))?;
        rename_if_present(&task_dir.join(ARTIFACTS_DIR), &pending.join(ARTIFACTS_DIR))?;
        subagent_state.move_task_root(
            &task_id,
            task_dir,
            &pending.join(subagents::SUBAGENTS_DIR),
        )?;
        sync_directory(&pending)?;
        sync_directory(task_dir)?;

        let now = crate::time::now_string();
        let mut task = metadata.task.clone();
        let reload_required = task.agent_session_id.is_some()
            && task.mark_native_session_reload_required(now.clone());
        let message_meta = MessageMeta {
            task_id: task_id.clone(),
            // Above every version a client may hold for the set-aside Chat.
            version: task.message_history_version.saturating_add(1),
            local_history_updated_at: now,
            ..MessageMeta::default()
        };
        task.message_history_version = message_meta.version;
        let projection = TaskProjection {
            task,
            messages: Vec::new(),
            message_meta,
            artifact_heads: Default::default(),
        };
        let storage_sequence = metadata.storage_sequence.saturating_add(1);
        split::publish_generation(
            task_dir,
            &projection,
            storage_sequence,
            &FaultInjector::disabled(),
        )?;
        finish(task_dir)?;
        Ok((projection, storage_sequence, reload_required))
    })();
    match result {
        Ok((projection, storage_sequence, reload_required)) => {
            crate::logging::info(
                "newer_chat_set_aside_finished",
                serde_json::json!({
                    "task_id": task_id,
                    "outcome": "rebuilt",
                    "reload_required": reload_required,
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
            Ok((projection, storage_sequence))
        }
        Err(error) => {
            crate::logging::warn(
                "newer_chat_set_aside_finished",
                serde_json::json!({
                    "task_id": task_id,
                    "outcome": "failed",
                    "error_kind": error.reason(),
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
            Err(error)
        }
    }
}

/// Settles a set-aside that a crash interrupted. Returns the Chat to rebuild
/// when the set-aside had not committed, because what it already moved would
/// otherwise go unnoticed. Finishes it when it had committed; the Task is then
/// readable, so a failure is logged and retried on the next load.
pub(super) fn interrupted(task_dir: &Path, task_id: &str) -> Option<NewerChat> {
    let pending = task_dir.join(PENDING_DIR);
    if !pending.is_dir() {
        return None;
    }
    let record = fs::read(pending.join(RECORD_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SetAsideRecord>(&bytes).ok());
    let current = split::load_task(task_dir).ok().flatten();
    let (outcome, error, resume) = match (record, current) {
        // Nothing moved before the record, so detection runs as usual and a
        // rebuild reuses the directory.
        (None, _) => ("unrecorded", None, None),
        (Some(_), None) => ("metadata_unavailable", None, None),
        (Some(record), Some(current)) if current.chat_snapshot == record.chat_snapshot => {
            ("resuming", None, Some(record.newer))
        }
        (Some(_), Some(_)) => match finish(task_dir) {
            Ok(()) => ("finished", None, None),
            Err(error) => ("failed", Some(error.reason()), None),
        },
    };
    crate::logging::info(
        "newer_chat_set_aside_resumed",
        serde_json::json!({
            "task_id": task_id,
            "outcome": outcome,
            "error_kind": error,
        }),
    );
    resume
}

/// Opens the pending directory and records the generation being replaced.
fn begin(
    task_dir: &Path,
    metadata: &split::TaskMetadata,
    newer: &NewerChat,
) -> Result<std::path::PathBuf, RuntimeError> {
    let pending = task_dir.join(PENDING_DIR);
    fs::create_dir_all(&pending)?;
    atomic::write_json(
        &pending.join(RECORD_FILE),
        &SetAsideRecord {
            newer: NewerChat {
                part: newer.part.clone(),
                version: newer.version,
                known: newer.known,
            },
            chat_snapshot: metadata.chat_snapshot.clone(),
            chat_journal: metadata.chat_journal.clone(),
        },
    )?;
    Ok(pending)
}

/// Moves the Chat files `task.json` no longer names into the pending directory,
/// then makes it the set-aside, replacing any earlier one.
fn finish(task_dir: &Path) -> Result<(), RuntimeError> {
    let pending = task_dir.join(PENDING_DIR);
    let metadata = split::load_task(task_dir)?
        .ok_or_else(|| RuntimeError::Storage("Split Task metadata is missing".to_string()))?;
    for entry in fs::read_dir(task_dir)? {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_chat_file = name.starts_with(split::CHAT_SNAPSHOT_FILE)
            || name.starts_with(split::CHAT_JOURNAL_FILE);
        if is_chat_file && name != metadata.chat_snapshot && name != metadata.chat_journal {
            fs::rename(task_dir.join(name), pending.join(name))?;
        }
    }
    sync_directory(&pending)?;
    let set_aside = task_dir.join(SET_ASIDE_DIR);
    match fs::remove_dir_all(&set_aside) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::rename(&pending, &set_aside)?;
    sync_directory(task_dir)
}

fn rename_if_present(from: &Path, to: &Path) -> Result<(), RuntimeError> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
#[path = "newer_chat_tests.rs"]
mod tests;
