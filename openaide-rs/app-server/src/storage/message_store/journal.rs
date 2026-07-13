use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, NormalizedMessage};
use crate::storage::records::StoredMessage;

const JOURNAL_FILE_NAME: &str = "message_journal.jsonl";

#[derive(Deserialize, Serialize)]
struct JournalRecord {
    sequence: u64,
    #[serde(flatten)]
    change: JournalChange,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum JournalChange {
    AppendMessage { message: Box<StoredMessage> },
    AppendText { identity: String, text: String },
}

/// Replays durable text deltas newer than the materialized history checkpoint.
pub(super) fn replay(
    task_dir: &Path,
    applied_sequence: u64,
    messages: &mut Vec<StoredMessage>,
) -> Result<u64, RuntimeError> {
    let path = task_dir.join(JOURNAL_FILE_NAME);
    if !path.exists() {
        return Ok(applied_sequence);
    }
    let bytes = fs::read(path)?;
    let complete_len = complete_records_len(&bytes);
    let mut latest_sequence = applied_sequence;
    for line in bytes[..complete_len]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let record: JournalRecord = serde_json::from_slice(line)?;
        latest_sequence = latest_sequence.max(record.sequence);
        if record.sequence <= applied_sequence {
            continue;
        }
        match record.change {
            JournalChange::AppendMessage { message } => {
                if messages
                    .iter()
                    .any(|stored| stored.chat.identity == message.chat.identity)
                {
                    return Err(RuntimeError::Conflict(
                        "Message journal appended a duplicate identity".to_string(),
                    ));
                }
                messages.push(*message);
            }
            JournalChange::AppendText { identity, text } => {
                apply_text_append(messages, &identity, &text)?;
            }
        }
    }
    Ok(latest_sequence)
}

pub(super) fn latest_sequence(task_dir: &Path) -> Result<u64, RuntimeError> {
    let path = task_dir.join(JOURNAL_FILE_NAME);
    if !path.exists() {
        return Ok(0);
    }
    let bytes = fs::read(path)?;
    let complete_len = complete_records_len(&bytes);
    let mut latest_sequence = 0;
    for line in bytes[..complete_len]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let record: JournalRecord = serde_json::from_slice(line)?;
        latest_sequence = latest_sequence.max(record.sequence);
    }
    Ok(latest_sequence)
}

/// Appends and syncs one small delta before a Task change can be published.
pub(super) fn append_text(
    task_dir: &Path,
    sequence: u64,
    identity: &str,
    text: &str,
) -> Result<u64, RuntimeError> {
    append_record(
        task_dir,
        &JournalRecord {
            sequence,
            change: JournalChange::AppendText {
                identity: identity.to_string(),
                text: text.to_string(),
            },
        },
    )
}

pub(super) fn append_message(
    task_dir: &Path,
    sequence: u64,
    message: &StoredMessage,
) -> Result<u64, RuntimeError> {
    append_record(
        task_dir,
        &JournalRecord {
            sequence,
            change: JournalChange::AppendMessage {
                message: Box::new(message.clone()),
            },
        },
    )
}

fn append_record(task_dir: &Path, record: &JournalRecord) -> Result<u64, RuntimeError> {
    fs::create_dir_all(task_dir)?;
    let path = task_dir.join(JOURNAL_FILE_NAME);
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(&path)?;
    let mut previous_len = file.metadata()?.len();
    if previous_len > 0 {
        file.seek(SeekFrom::End(-1))?;
        let mut last_byte = [0_u8; 1];
        file.read_exact(&mut last_byte)?;
        if last_byte[0] != b'\n' {
            let bytes = fs::read(&path)?;
            let complete_len = complete_records_len(&bytes) as u64;
            file.set_len(complete_len)?;
            crate::logging::warn(
                "message_journal_incomplete_tail_discarded",
                serde_json::json!({
                    "discarded_bytes": previous_len.saturating_sub(complete_len),
                }),
            );
            previous_len = complete_len;
        }
    }
    let mut bytes = serde_json::to_vec(record)?;
    bytes.push(b'\n');
    file.write_all(&bytes)?;
    file.sync_all().ok();
    Ok(previous_len)
}

fn complete_records_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or_default()
}

pub(super) fn truncate(task_dir: &Path, len: u64) -> Result<(), RuntimeError> {
    let path = task_dir.join(JOURNAL_FILE_NAME);
    if len == 0 {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(RuntimeError::from(error)),
        }
    }
    File::options().write(true).open(path)?.set_len(len)?;
    Ok(())
}

pub(super) fn path(task_dir: &Path) -> std::path::PathBuf {
    task_dir.join(JOURNAL_FILE_NAME)
}

pub(super) fn remove(task_dir: &Path) -> Result<(), RuntimeError> {
    match fs::remove_file(task_dir.join(JOURNAL_FILE_NAME)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::from(error)),
    }
}

fn apply_text_append(
    messages: &mut [StoredMessage],
    identity: &str,
    chunk: &str,
) -> Result<(), RuntimeError> {
    let stored = messages
        .iter_mut()
        .find(|stored| stored.chat.identity == identity)
        .ok_or_else(|| {
            RuntimeError::Conflict("Agent text journal target is missing".to_string())
        })?;
    let NormalizedMessage::AgentMessage { parts, .. } = &mut stored.chat.message else {
        return Err(RuntimeError::Conflict(
            "Agent text journal target changed content channel".to_string(),
        ));
    };
    let Some(AgentMessagePart::Text { text }) = parts.last_mut() else {
        return Err(RuntimeError::Conflict(
            "Agent text journal target has no text part".to_string(),
        ));
    };
    text.push_str(chunk);
    Ok(())
}
