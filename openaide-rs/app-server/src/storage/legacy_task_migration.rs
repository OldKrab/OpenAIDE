//! One-way import of the file-backed Task store used before July 21, 2026.
//! Original files move into the imported Task only after durable verification,
//! so interruption is retryable and ordinary Task deletion/reset owns the backup.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityToolDetails, AgentMessagePart, NormalizedMessage};

use super::atomic::sync_directory;
use super::records::{MessageMeta, StoredMessage, TaskRecord};
use super::task_journal::{TaskJournalStore, TaskProjection, TaskWrite, ToolArtifactReplacement};

pub(super) fn import(root: &Path, store: &TaskJournalStore) -> Result<(), RuntimeError> {
    let source = root.join("tasks");
    if !source.try_exists()? {
        return Ok(());
    }
    require_directory(&source)?;
    for entry in fs::read_dir(&source)? {
        let entry = entry?;
        let task_id = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| migration_error("invalid Task directory identity"))?;
        super::id::validate_task_id(&task_id)
            .map_err(|_| migration_error("invalid Task directory identity"))?;
        require_directory(&entry.path())?;
        let started = std::time::Instant::now();
        crate::logging::info(
            "legacy_task_migration_started",
            serde_json::json!({ "task_id": task_id }),
        );
        let result = import_task(root, &entry.path(), &task_id, store);
        crate::logging::info(
            "legacy_task_migration_finished",
            serde_json::json!({
                "task_id": task_id,
                "outcome": if result.is_ok() { "imported" } else { "original_preserved" },
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        result?;
    }
    Ok(())
}

fn import_task(
    root: &Path,
    source: &Path,
    task_id: &str,
    store: &TaskJournalStore,
) -> Result<(), RuntimeError> {
    let projection = read_projection(source, task_id)?;
    let artifacts = read_artifacts(source, task_id)?;
    match store.load(task_id) {
        Ok(_) => {} // A crash may have followed the commit but preceded the source move.
        Err(RuntimeError::TaskNotFound(_)) => {
            store
                .submit(TaskWrite::barrier_create_with_artifacts(
                    projection.clone(),
                    artifacts.clone(),
                ))?
                .wait()?;
        }
        Err(_) => return Err(migration_error("existing destination Task is unavailable")),
    }
    let mut expected = projection;
    expected.task.clear_process_local_agent_state();
    expected.artifact_heads = artifacts
        .iter()
        .map(|artifact| (artifact.artifact_id.clone(), 1))
        .collect();
    let mut actual = store.load(task_id)?;
    actual.task.clear_process_local_agent_state();
    if serde_json::to_value(&actual)? != serde_json::to_value(&expected)? {
        return Err(migration_error(
            "existing destination Task conflicts with legacy history",
        ));
    }
    for artifact in artifacts {
        let stored = store.load_tool_artifact(task_id, &artifact.artifact_id)?;
        if serde_json::to_value(stored.details)? != serde_json::to_value(Some(artifact.details))? {
            return Err(migration_error(
                "destination Tool detail failed verification",
            ));
        }
    }
    let target = root.join("task-store-v1/tasks").join(task_id);
    let backup = target.join("legacy-files");
    if backup.try_exists()? {
        return Err(migration_error(
            "both a legacy Task and its imported backup exist",
        ));
    }
    fs::rename(source, backup)?;
    sync_directory(&target)?;
    sync_directory(source.parent().expect("legacy Task has a parent"))?;
    Ok(())
}

fn read_projection(source: &Path, task_id: &str) -> Result<TaskProjection, RuntimeError> {
    let task: TaskRecord = decode(
        &required_file(&source.join("task.json"))?,
        "invalid Task metadata",
    )?;
    if task.task_id != task_id {
        return Err(migration_error("Task metadata identity mismatch"));
    }
    let mut messages = Vec::new();
    let mut checkpoint = 0;
    if let Some(bytes) = optional_file(&source.join("messages.jsonl"))? {
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        {
            let value: serde_json::Value = decode(line, "invalid materialized Chat")?;
            if value.get("record_type").and_then(serde_json::Value::as_str) == Some("message_base")
            {
                checkpoint = value
                    .get("journal_sequence")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| migration_error("invalid Chat checkpoint"))?;
            } else {
                messages.push(
                    serde_json::from_value(value)
                        .map_err(|_| migration_error("unsupported Chat message"))?,
                );
            }
        }
    }
    replay_journal(source, checkpoint, &mut messages)?;
    let mut identities = HashSet::new();
    let mut message_ids = HashSet::new();
    let mut sequences = HashSet::new();
    for stored in &messages {
        if !identities.insert(&stored.chat.identity)
            || !message_ids.insert(&stored.chat.message_id)
            || !sequences.insert(stored.sequence)
        {
            return Err(migration_error("duplicate Chat message identity"));
        }
    }
    let mut message_meta: MessageMeta = match optional_file(&source.join("message_meta.json"))? {
        Some(bytes) => decode(&bytes, "invalid Chat metadata")?,
        None => MessageMeta {
            task_id: task_id.to_string(),
            version: task.message_history_version.max(messages.len() as u64),
            local_history_updated_at: task.updated_at.clone(),
            ..MessageMeta::default()
        },
    };
    if message_meta.task_id != task_id {
        return Err(migration_error("Chat metadata identity mismatch"));
    }
    // The old writer published Chat before its separate metadata file. Rebuild
    // derived navigation facts from validated history after a crash; the stored
    // version and clock remain authoritative because their next values were not
    // committed. This also keeps interrupted migration retries deterministic.
    message_meta.message_count = messages.len() as u64;
    message_meta.first_cursor = messages.first().map(|stored| stored.chat.cursor.clone());
    message_meta.last_cursor = messages.last().map(|stored| stored.chat.cursor.clone());
    Ok(TaskProjection {
        task,
        messages,
        message_meta,
        artifact_heads: HashMap::new(),
    })
}

#[derive(Deserialize)]
struct LegacyJournalRecord {
    sequence: u64,
    #[serde(flatten)]
    change: LegacyJournalChange,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum LegacyJournalChange {
    AppendMessage { message: Box<StoredMessage> },
    AppendText { identity: String, text: String },
}

fn replay_journal(
    source: &Path,
    checkpoint: u64,
    messages: &mut Vec<StoredMessage>,
) -> Result<(), RuntimeError> {
    let Some(bytes) = optional_file(&source.join("message_journal.jsonl"))? else {
        return Ok(());
    };
    // The old writer committed newline-terminated records and ignored a partial
    // tail. Preserve its bytes in the backup without inventing accepted output.
    let complete = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let mut sequence = checkpoint;
    for line in bytes[..complete]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let record: LegacyJournalRecord = decode(line, "invalid Chat journal record")?;
        if record.sequence <= checkpoint {
            continue;
        }
        if sequence.checked_add(1) != Some(record.sequence) {
            return Err(migration_error("Chat journal sequence gap"));
        }
        sequence = record.sequence;
        match record.change {
            LegacyJournalChange::AppendMessage { message } => messages.push(*message),
            LegacyJournalChange::AppendText {
                identity,
                text: chunk,
            } => {
                let stored = messages
                    .iter_mut()
                    .find(|stored| stored.chat.identity == identity)
                    .ok_or_else(|| migration_error("Chat journal text target is missing"))?;
                let NormalizedMessage::AgentMessage { parts, .. } = &mut stored.chat.message else {
                    return Err(migration_error(
                        "Chat journal text target is not an Agent message",
                    ));
                };
                let Some(AgentMessagePart::Text { text }) = parts.last_mut() else {
                    return Err(migration_error("Chat journal text target has no text part"));
                };
                text.push_str(&chunk);
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct LegacyArtifact {
    task_id: String,
    artifact_id: String,
    details: ActivityToolDetails,
}

fn read_artifacts(
    source: &Path,
    task_id: &str,
) -> Result<Vec<ToolArtifactReplacement>, RuntimeError> {
    let directory = source.join("tool-artifacts");
    if !directory.try_exists()? {
        return Ok(Vec::new());
    }
    require_directory(&directory)?;
    let mut artifacts = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("json")
        {
            continue;
        }
        let artifact: LegacyArtifact =
            decode(&required_file(&entry.path())?, "invalid Tool artifact")?;
        if artifact.task_id != task_id
            || entry.path().file_stem().and_then(|stem| stem.to_str())
                != Some(&artifact.artifact_id)
        {
            return Err(migration_error("Tool artifact identity mismatch"));
        }
        artifacts.push(ToolArtifactReplacement {
            artifact_id: artifact.artifact_id,
            details: artifact.details,
        });
    }
    Ok(artifacts)
}

fn required_file(path: &Path) -> Result<Vec<u8>, RuntimeError> {
    optional_file(path)?.ok_or_else(|| migration_error("required legacy file is missing"))
}

fn optional_file(path: &Path) -> Result<Option<Vec<u8>>, RuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => fs::read(path)
            .map(Some)
            .map_err(|_| migration_error("legacy file cannot be read")),
        Ok(_) => Err(migration_error("legacy file is a symlink or special entry")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(migration_error("legacy file cannot be inspected")),
    }
}

fn require_directory(path: &Path) -> Result<(), RuntimeError> {
    if fs::symlink_metadata(path)?.file_type().is_dir() {
        Ok(())
    } else {
        Err(migration_error(
            "legacy directory is a symlink or special entry",
        ))
    }
}

fn decode<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    reason: &'static str,
) -> Result<T, RuntimeError> {
    serde_json::from_slice(bytes).map_err(|_| migration_error(reason))
}

fn migration_error(reason: &'static str) -> RuntimeError {
    RuntimeError::Storage(format!(
        "Legacy Task migration failed: {reason}; original files were preserved"
    ))
}
