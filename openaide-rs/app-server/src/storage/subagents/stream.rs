//! A Subagent History stored as a Chat stream of its Task (ADR-0076).
//!
//! A small head names the current generation: a snapshot and a journal of the
//! changes after it, written in the Task journal's checksummed frames. Compaction
//! publishes a new generation through the head before it removes the old files,
//! so the files the head names are always authoritative.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, AgentPlan, NormalizedMessage};
use crate::storage::records::{MessageMeta, StoredMessage};
use crate::storage::task_journal::frame::{self, FramedRecord, ReplayedFrames};

pub(super) const STREAM_HEAD_FILE: &str = "stream.json";
const SNAPSHOT_FILE: &str = "chat.snapshot";
const JOURNAL_FILE: &str = "chat.journal";
const FRAME_FORMAT_VERSION: u16 = 1;
/// An App Server that does not know a newer stream schema rejects it unread.
const STREAM_SCHEMA_VERSION: u16 = 1;
/// ADR-0029's thresholds: replay stays short and rewrites stay proportional.
pub(super) const COMPACT_AFTER_FRAMES: u64 = 128;
const MIN_COMPACTION_BYTES: u64 = 1 << 20;

/// One Subagent's Chat; each committed change advances `revision` by one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SubagentChat {
    pub(super) revision: u64,
    pub(super) messages: Vec<StoredMessage>,
    pub(super) message_meta: MessageMeta,
    pub(super) current_plan: Option<AgentPlan>,
}

/// What one history mutation changed, so the journal records only that.
pub(super) enum HistoryChange {
    /// Streamed Agent text appended to the identified message.
    AppendText { identity: String, text: String },
    /// The message with this identity was added or replaced.
    Upsert(String),
    /// The current plan was replaced or cleared.
    Plan,
}

/// One journaled change. Message changes carry the history timestamp they
/// set, so replay rebuilds the same metadata.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(super) enum StreamOperation {
    AppendText {
        identity: String,
        text: String,
        local_history_updated_at: String,
    },
    UpsertMessage {
        message: Box<StoredMessage>,
        local_history_updated_at: String,
    },
    /// Replaces the current plan; `None` clears it.
    SetPlan { plan: Option<AgentPlan> },
}

impl StreamOperation {
    /// Describes `change`, already applied to `chat`, as its journal operation.
    pub(super) fn describe(
        chat: &SubagentChat,
        change: &HistoryChange,
    ) -> Result<Self, RuntimeError> {
        let local_history_updated_at = chat.message_meta.local_history_updated_at.clone();
        Ok(match change {
            HistoryChange::AppendText { identity, text } => Self::AppendText {
                identity: identity.clone(),
                text: text.clone(),
                local_history_updated_at,
            },
            HistoryChange::Upsert(identity) => Self::UpsertMessage {
                message: Box::new(
                    chat.messages
                        .iter()
                        .find(|stored| &stored.chat.identity == identity)
                        .ok_or_else(|| {
                            RuntimeError::Storage(
                                "Subagent message change has no stored message".to_string(),
                            )
                        })?
                        .clone(),
                ),
                local_history_updated_at,
            },
            HistoryChange::Plan => Self::SetPlan {
                plan: chat.current_plan.clone(),
            },
        })
    }

    fn apply(self, chat: &mut SubagentChat) -> Result<(), RuntimeError> {
        match self {
            Self::AppendText {
                identity,
                text,
                local_history_updated_at,
            } => {
                append_text(chat, &identity, text)?;
                set_history_meta(chat, local_history_updated_at);
            }
            Self::UpsertMessage {
                message,
                local_history_updated_at,
            } => {
                match chat
                    .messages
                    .iter_mut()
                    .find(|stored| stored.chat.identity == message.chat.identity)
                {
                    Some(stored) => *stored = *message,
                    None => chat.messages.push(*message),
                }
                set_history_meta(chat, local_history_updated_at);
            }
            Self::SetPlan { plan } => chat.current_plan = plan,
        }
        chat.revision = chat.revision.saturating_add(1);
        Ok(())
    }
}

fn append_text(chat: &mut SubagentChat, identity: &str, text: String) -> Result<(), RuntimeError> {
    let parts = chat
        .messages
        .iter_mut()
        .find(|stored| stored.chat.identity == identity)
        .and_then(|stored| match &mut stored.chat.message {
            NormalizedMessage::AgentMessage { parts, .. } => Some(parts),
            _ => None,
        })
        .ok_or_else(|| {
            RuntimeError::Storage("Subagent text append has no Agent message".to_string())
        })?;
    if let Some(AgentMessagePart::Text { text: last }) = parts.last_mut() {
        last.push_str(&text);
    } else {
        parts.push(AgentMessagePart::Text { text });
    }
    Ok(())
}

/// Advances the history metadata after a message change, keeping its timestamp
/// strictly increasing.
pub(super) fn advance_history_meta(chat: &mut SubagentChat) {
    let previous = chat
        .message_meta
        .local_history_updated_at
        .parse::<u128>()
        .unwrap_or_default();
    let now = crate::time::now_string()
        .parse::<u128>()
        .unwrap_or_default();
    set_history_meta(chat, now.max(previous.saturating_add(1)).to_string());
}

fn set_history_meta(chat: &mut SubagentChat, local_history_updated_at: String) {
    let meta = &mut chat.message_meta;
    meta.version = meta.version.saturating_add(1);
    meta.message_count = chat.messages.len() as u64;
    meta.local_history_updated_at = local_history_updated_at;
    meta.first_cursor = chat
        .messages
        .first()
        .map(|stored| stored.chat.cursor.clone());
    meta.last_cursor = chat
        .messages
        .last()
        .map(|stored| stored.chat.cursor.clone());
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamHead {
    schema_version: u16,
    snapshot: String,
    journal: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotFile<C> {
    schema_version: u16,
    #[serde(flatten)]
    chat: C,
}

#[derive(Serialize, Deserialize)]
struct StreamFrame {
    format_version: u16,
    schema_version: u16,
    sequence: u64,
    /// The history revision after the frame; each operation is one revision.
    revision: u64,
    operations: Vec<StreamOperation>,
}

impl FramedRecord for StreamFrame {
    fn decode(payload: &[u8]) -> Result<Self, RuntimeError> {
        #[derive(Deserialize)]
        struct Envelope {
            schema_version: u16,
        }
        let envelope: Envelope = serde_json::from_slice(payload)?;
        validate_schema(envelope.schema_version)?;
        Ok(serde_json::from_slice(payload)?)
    }

    fn format_version(&self) -> u16 {
        self.format_version
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }
}

/// The current generation of an open stream and what decides its compaction.
#[derive(Debug)]
pub(super) struct StreamFiles {
    journal: String,
    snapshot_bytes: u64,
    journal_bytes: u64,
    journal_frames: u64,
}

impl StreamFiles {
    pub(super) fn compaction_due(&self) -> bool {
        self.journal_frames >= COMPACT_AFTER_FRAMES
            || (self.journal_bytes >= self.snapshot_bytes
                && self.journal_bytes >= MIN_COMPACTION_BYTES)
    }

    pub(super) fn journal_frames(&self) -> u64 {
        self.journal_frames
    }
}

pub(super) fn exists(dir: &Path) -> bool {
    dir.join(STREAM_HEAD_FILE).is_file()
}

/// Loads the generation the head names. A journal whose creation a crash
/// interrupted holds no committed frame and is removed, so the next append
/// can create it again.
pub(super) fn load(dir: &Path) -> Result<(SubagentChat, StreamFiles), RuntimeError> {
    let head: StreamHead = serde_json::from_slice(&fs::read(dir.join(STREAM_HEAD_FILE))?)?;
    validate_schema(head.schema_version)?;
    let snapshot_bytes = fs::read(dir.join(&head.snapshot))?;
    let snapshot: SnapshotFile<serde_json::Value> = serde_json::from_slice(&snapshot_bytes)?;
    validate_schema(snapshot.schema_version)?;
    let mut chat: SubagentChat = serde_json::from_value(snapshot.chat)?;
    let journal_path = dir.join(&head.journal);
    let mut journal_frames = 0;
    let journal_bytes = match fs::metadata(&journal_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
        Ok(metadata) => {
            let replayed: Option<ReplayedFrames<StreamFrame>> =
                if metadata.len() < frame::FILE_HEADER_LEN as u64 {
                    None
                } else {
                    Some(frame::replay(&journal_path)?)
                };
            match replayed {
                Some(replayed) if replayed.frame_count > 0 => {
                    journal_frames = replayed.frame_count as u64;
                    for frame in replayed.frames {
                        for operation in frame.operations {
                            operation.apply(&mut chat)?;
                        }
                        if chat.revision != frame.revision {
                            return Err(RuntimeError::Storage(
                                "Subagent stream revision is inconsistent".to_string(),
                            ));
                        }
                    }
                    fs::metadata(&journal_path)?.len()
                }
                _ => {
                    fs::remove_file(&journal_path)?;
                    crate::storage::atomic::sync_directory(dir)?;
                    0
                }
            }
        }
    };
    Ok((
        chat,
        StreamFiles {
            journal: head.journal,
            snapshot_bytes: snapshot_bytes.len() as u64,
            journal_bytes,
            journal_frames,
        },
    ))
}

/// Appends one frame of a batch's operations with one sync; `chat` already
/// holds them.
pub(super) fn append(
    dir: &Path,
    files: &mut StreamFiles,
    chat: &SubagentChat,
    operations: Vec<StreamOperation>,
) -> Result<(), RuntimeError> {
    let frame = StreamFrame {
        format_version: FRAME_FORMAT_VERSION,
        schema_version: STREAM_SCHEMA_VERSION,
        sequence: files.journal_frames.saturating_add(1),
        revision: chat.revision,
        operations,
    };
    let path = dir.join(&files.journal);
    if files.journal_frames == 0 {
        frame::create(&path, &frame)?;
    } else {
        frame::append(&path, &frame)?;
    }
    files.journal_frames += 1;
    files.journal_bytes = fs::metadata(&path)?.len();
    Ok(())
}

/// Publishes `chat` as a new generation, then removes every other generation.
/// Until the head is replaced the previous generation stays authoritative.
pub(super) fn publish_generation(
    dir: &Path,
    chat: &SubagentChat,
) -> Result<StreamFiles, RuntimeError> {
    let generation = uuid::Uuid::new_v4().simple();
    let snapshot = format!("{SNAPSHOT_FILE}.{generation}");
    let journal = format!("{JOURNAL_FILE}.{generation}");
    let bytes = serde_json::to_vec(&SnapshotFile {
        schema_version: STREAM_SCHEMA_VERSION,
        chat,
    })?;
    crate::storage::atomic::write_bytes(&dir.join(&snapshot), &bytes)?;
    crate::storage::atomic::write_json(
        &dir.join(STREAM_HEAD_FILE),
        &StreamHead {
            schema_version: STREAM_SCHEMA_VERSION,
            snapshot: snapshot.clone(),
            journal: journal.clone(),
        },
    )?;
    remove_other_generations(dir, &snapshot, &journal);
    Ok(StreamFiles {
        journal,
        snapshot_bytes: bytes.len() as u64,
        journal_bytes: 0,
        journal_frames: 0,
    })
}

/// Old generations are garbage once the head names a new one; a failure here
/// leaves files a later compaction removes.
fn remove_other_generations(dir: &Path, snapshot: &str, journal: &str) {
    let removed = (|| -> Result<usize, RuntimeError> {
        let mut removed = 0;
        for entry in fs::read_dir(dir)? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else { continue };
            let generation_file = name.starts_with(SNAPSHOT_FILE) || name.starts_with(JOURNAL_FILE);
            if generation_file && name != snapshot && name != journal {
                fs::remove_file(dir.join(name))?;
                removed += 1;
            }
        }
        if removed > 0 {
            crate::storage::atomic::sync_directory(dir)?;
        }
        Ok(removed)
    })();
    if let Err(error) = removed {
        crate::logging::warn(
            "subagent_stream_cleanup_failed",
            serde_json::json!({
                "subagent_id": dir.file_name().map(|name| name.to_string_lossy()),
                "error_kind": error.reason(),
            }),
        );
    }
}

fn validate_schema(version: u16) -> Result<(), RuntimeError> {
    if version == STREAM_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(RuntimeError::Storage(
            "Unsupported Subagent stream schema".to_string(),
        ))
    }
}

#[cfg(test)]
fn test_head(dir: &Path) -> StreamHead {
    serde_json::from_slice(&fs::read(dir.join(STREAM_HEAD_FILE)).unwrap()).unwrap()
}

/// The journal of the generation the head names; it may not exist yet.
#[cfg(test)]
pub(super) fn journal_path(dir: &Path) -> std::path::PathBuf {
    dir.join(test_head(dir).journal)
}

#[cfg(test)]
pub(super) fn snapshot_path(dir: &Path) -> std::path::PathBuf {
    dir.join(test_head(dir).snapshot)
}

/// The operation names of each journal frame, in order.
#[cfg(test)]
pub(super) fn journaled_operations(dir: &Path) -> Vec<Vec<&'static str>> {
    let path = journal_path(dir);
    if !path.exists() {
        return Vec::new();
    }
    frame::replay::<StreamFrame>(&path)
        .unwrap()
        .frames
        .into_iter()
        .map(|frame| {
            frame
                .operations
                .iter()
                .map(|operation| match operation {
                    StreamOperation::AppendText { .. } => "append_text",
                    StreamOperation::UpsertMessage { .. } => "upsert_message",
                    StreamOperation::SetPlan { .. } => "set_plan",
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
