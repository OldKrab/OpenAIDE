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

// Full frames are the original schema-1 journal representation and remain readable.
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

pub(super) fn load_history(dir: &Path) -> Result<HistoryFile, RuntimeError> {
    let bytes = fs::read(dir.join(HISTORY_SNAPSHOT_FILE))?;
    let mut history: HistoryFile = serde_json::from_slice(&bytes)?;
    validate_schema(history.schema_version)?;
    let snapshot_revision = history.revision;
    let journal = dir.join(HISTORY_JOURNAL_FILE);
    let file = fs::File::open(&journal)?;
    let mut reader = BufReader::new(file);
    let mut complete_bytes = 0;
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
    Ok(history)
}

pub(super) fn append_history_frame(
    path: &Path,
    history: &HistoryFile,
    append: Option<TextAppend>,
) -> Result<(), RuntimeError> {
    let frame = match append {
        Some(append_text) => HistoryFrame::AppendText {
            revision: history.revision,
            append_text,
            message_meta: history.message_meta.clone(),
        },
        None => HistoryFrame::Full {
            revision: history.revision,
            history,
        },
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
