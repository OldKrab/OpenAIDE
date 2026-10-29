use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, AgentPlan, NormalizedMessage};
use crate::storage::records::{MessageMeta, StoredMessage};

use super::{validate_schema, SCHEMA_VERSION};

pub(super) const HISTORY_SNAPSHOT_FILE: &str = "history.snapshot";
pub(super) const HISTORY_JOURNAL_FILE: &str = "history.journal";
pub(super) const COMPACT_AFTER_FRAMES: u64 = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HistoryFile {
    pub(super) schema_version: u16,
    pub(super) revision: u64,
    pub(super) journal_frames: u64,
    pub(super) messages: Vec<StoredMessage>,
    pub(super) message_meta: MessageMeta,
    pub(super) current_plan: Option<AgentPlan>,
}

impl Default for HistoryFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            revision: 0,
            journal_frames: 0,
            messages: Vec::new(),
            message_meta: MessageMeta::default(),
            current_plan: None,
        }
    }
}

// Full frames are the original schema-1 journal representation and remain readable;
// current writers publish such a change as the snapshot instead.
// Streamed Agent text uses a semantic append so each chunk does not rewrite all
// previous output. Its metadata is committed in the same newline-delimited frame.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
enum HistoryFrame<H> {
    Full {
        revision: u64,
        history: H,
    },
    AppendText {
        revision: u64,
        append_text: TextAppend,
        message_meta: MessageMeta,
    },
}

impl<H> HistoryFrame<H> {
    fn revision(&self) -> u64 {
        match self {
            Self::Full { revision, .. } | Self::AppendText { revision, .. } => *revision,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct TextAppend {
    pub(super) identity: String,
    pub(super) role: AgentMessageRole,
    pub(super) text: String,
}

impl TextAppend {
    fn apply(self, history: &mut HistoryFile) -> Result<(), RuntimeError> {
        let Some(stored) = history
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == self.identity)
        else {
            return Err(RuntimeError::Storage(
                "Subagent text append has no stored message".to_string(),
            ));
        };
        let NormalizedMessage::AgentMessage { role, parts, .. } = &mut stored.chat.message else {
            return Err(RuntimeError::Storage(
                "Subagent text append has a different message kind".to_string(),
            ));
        };
        if *role != self.role {
            return Err(RuntimeError::Storage(
                "Subagent text append has a different message role".to_string(),
            ));
        }
        if let Some(AgentMessagePart::Text { text }) = parts.last_mut() {
            text.push_str(&self.text);
        } else {
            parts.push(AgentMessagePart::Text { text: self.text });
        }
        Ok(())
    }
}

/// What replaying a journal found beyond the snapshot.
#[derive(Debug, Default)]
pub(super) struct JournalReplay {
    /// The journal's complete bytes.
    pub(super) bytes: u64,
    /// Frames that repeat history the snapshot or a later frame holds: full frames
    /// of the schema-1 writer, or a prefix an interrupted compaction left behind.
    pub(super) repeated_frames: u64,
}

fn load_history(dir: &Path) -> Result<(HistoryFile, JournalReplay), RuntimeError> {
    let bytes = fs::read(dir.join(HISTORY_SNAPSHOT_FILE))?;
    let mut history: HistoryFile = serde_json::from_slice(&bytes)?;
    validate_schema(history.schema_version)?;
    let snapshot_revision = history.revision;
    let journal = dir.join(HISTORY_JOURNAL_FILE);
    let file = fs::File::open(&journal)?;
    let mut reader = BufReader::new(file);
    let mut complete_bytes = 0;
    let mut repeated_frames = 0;
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if !line.ends_with(b"\n") {
            // A frame is published only with its newline. Discard an interrupted
            // final write before the next append; completed malformed frames fail.
            let file = OpenOptions::new().write(true).open(&journal)?;
            file.set_len(complete_bytes)?;
            file.sync_all()?;
            crate::logging::warn(
                "subagent_history_incomplete_tail_discarded",
                serde_json::json!({ "discarded_bytes": line.len() }),
            );
            break;
        }
        complete_bytes += line.len() as u64;
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let frame: HistoryFrame<HistoryFile> = serde_json::from_slice(&line)?;
        let revision = frame.revision();
        if revision <= snapshot_revision {
            // Snapshot publication precedes journal replacement. An interrupted
            // compaction may leave an already-materialized journal prefix.
            repeated_frames += 1;
            continue;
        }
        if revision != history.revision.saturating_add(1) {
            return Err(RuntimeError::Storage(
                "Subagent history journal sequence is invalid".to_string(),
            ));
        }
        match frame {
            HistoryFrame::Full { history: next, .. } => {
                validate_schema(next.schema_version)?;
                if revision != next.revision {
                    return Err(RuntimeError::Storage(
                        "Subagent history frame revision is inconsistent".to_string(),
                    ));
                }
                history = next;
                repeated_frames += 1;
            }
            HistoryFrame::AppendText {
                append_text,
                message_meta,
                ..
            } => {
                append_text.apply(&mut history)?;
                history.message_meta = message_meta;
                history.revision = revision;
                history.journal_frames = history.journal_frames.saturating_add(1);
            }
        }
    }
    // codex-acp previously projected the encrypted causal-root placeholder as if it
    // were child-authored User text. The identity is adapter-owned, so suppressing
    // that exact legacy shape does not discard genuine provider-neutral history.
    history.messages.retain(|stored| {
        let identity = stored.chat.identity.as_str();
        !(identity.contains(":user:collab:") && identity.ends_with(":prompt"))
    });
    for stored in &mut history.messages {
        if !stored.chat.identity.contains(":codex:") {
            continue;
        }
        if let NormalizedMessage::Activity { steps, .. } = &mut stored.chat.message {
            if let [crate::protocol::model::ActivityStep::Text { level, .. }] = steps.as_mut_slice()
            {
                *level = Some("agent_boundary".to_string());
            }
        }
    }
    Ok((
        history,
        JournalReplay {
            bytes: complete_bytes,
            repeated_frames,
        },
    ))
}

/// Loads a history in its current stored form, upgrading what older writers left:
/// - A journal of full-history frames (schema-1 writers) repeats the whole history
///   in every frame and is replayed on every load; it is published as the snapshot.
/// - A history written before ADR-0075 still holds inline images; they move into
///   the Task's image files. References stored without dimensions gain them.
///
/// An upgrade publishes the snapshot and resets the journal, so it happens once.
/// Returns whether the history was rewritten. Callers hold the Subagent write lock.
pub(super) fn load_upgraded_history(
    task_dir: &Path,
    dir: &Path,
) -> Result<(HistoryFile, bool), RuntimeError> {
    let (mut history, replay) = load_history(dir)?;
    let images = crate::storage::chat_images::has_inline_images(&history.messages)
        || crate::storage::chat_images::has_references_without_dimensions(&history.messages);
    if replay.repeated_frames == 0 && !images {
        return Ok((history, false));
    }
    let started_at = std::time::Instant::now();
    let (moved, described) = if images {
        let moved =
            crate::storage::chat_images::externalize_messages(task_dir, &mut history.messages)?;
        let described =
            crate::storage::chat_images::add_missing_dimensions(task_dir, &mut history.messages);
        (moved, described)
    } else {
        (0, 0)
    };
    // A file whose header cannot be read leaves its reference without a size;
    // rewriting the history for it alone would repeat on every load.
    if replay.repeated_frames == 0 && moved + described == 0 {
        return Ok((history, false));
    }
    history.journal_frames = 0;
    crate::storage::atomic::write_json(&dir.join(HISTORY_SNAPSHOT_FILE), &history)?;
    durable_create_empty(&dir.join(HISTORY_JOURNAL_FILE))?;
    crate::logging::info(
        "subagent_history_upgraded",
        serde_json::json!({
            "task_id": task_dir.file_name().map(|name| name.to_string_lossy()),
            "subagent_id": dir.file_name().map(|name| name.to_string_lossy()),
            "repeated_frames": replay.repeated_frames,
            "journal_bytes": replay.bytes,
            "moved_image_count": moved,
            "described_image_count": described,
            "message_count": history.messages.len(),
            "duration_ms": started_at.elapsed().as_millis(),
        }),
    );
    Ok((history, true))
}

/// Appends streamed text. Other changes publish the snapshot instead of a full frame.
pub(super) fn append_history_frame(
    path: &Path,
    history: &HistoryFile,
    append_text: TextAppend,
) -> Result<(), RuntimeError> {
    let frame = HistoryFrame::<&HistoryFile>::AppendText {
        revision: history.revision,
        append_text,
        message_meta: history.message_meta.clone(),
    };
    let bytes = serde_json::to_vec(&frame)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

pub(super) fn advance_history_meta(history: &mut HistoryFile) {
    let previous = history
        .message_meta
        .local_history_updated_at
        .parse::<u128>()
        .unwrap_or_default();
    let now = crate::time::now_string()
        .parse::<u128>()
        .unwrap_or_default();
    history.message_meta.version = history.message_meta.version.saturating_add(1);
    history.message_meta.message_count = history.messages.len() as u64;
    history.message_meta.local_history_updated_at = now.max(previous.saturating_add(1)).to_string();
    history.message_meta.first_cursor = history
        .messages
        .first()
        .map(|message| message.chat.cursor.clone());
    history.message_meta.last_cursor = history
        .messages
        .last()
        .map(|message| message.chat.cursor.clone());
}

pub(super) fn durable_create_empty(path: &Path) -> Result<(), RuntimeError> {
    // Snapshot publication happens first. Replacing the journal atomically
    // avoids leaving a partially truncated file when the reset itself fails.
    crate::storage::atomic::write_bytes(path, &[])
}
