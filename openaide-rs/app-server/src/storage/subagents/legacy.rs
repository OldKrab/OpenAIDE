//! Reads Subagent histories in the ADR-0046 format and migrates each to a Chat
//! stream (ADR-0076) on its first access. Nothing here writes the old format.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Deserialize;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, AgentPlan, NormalizedMessage};
use crate::storage::records::{MessageMeta, StoredMessage};

use super::stream::{self, StreamFiles, SubagentChat};

pub(super) const HISTORY_SNAPSHOT_FILE: &str = "history.snapshot";
const HISTORY_JOURNAL_FILE: &str = "history.journal";
/// Schema 1 journals hold full-history and text frames; schema 2 adds message
/// and plan frames.
const HISTORY_SCHEMA_VERSION: u16 = 2;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryFile {
    schema_version: u16,
    revision: u64,
    messages: Vec<StoredMessage>,
    message_meta: MessageMeta,
    current_plan: Option<AgentPlan>,
}

/// One newline-delimited journal frame. Schema 1 writers journaled the full
/// history; schema 2 writers journaled only what changed.
#[derive(Debug, Deserialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
enum HistoryFrame {
    Full {
        revision: u64,
        history: HistoryFile,
    },
    AppendText {
        revision: u64,
        append_text: TextAppend,
        message_meta: MessageMeta,
    },
    Upsert {
        revision: u64,
        upsert: Box<StoredMessage>,
        message_meta: MessageMeta,
    },
    SetPlan {
        revision: u64,
        set_plan: PlanChange,
    },
}

/// A required wrapper, so a frame without `setPlan` cannot read as a cleared plan.
#[derive(Debug, Deserialize)]
struct PlanChange {
    plan: Option<AgentPlan>,
}

impl HistoryFrame {
    fn revision(&self) -> u64 {
        match self {
            Self::Full { revision, .. }
            | Self::AppendText { revision, .. }
            | Self::Upsert { revision, .. }
            | Self::SetPlan { revision, .. } => *revision,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TextAppend {
    identity: String,
    role: AgentMessageRole,
    text: String,
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

pub(super) fn exists(dir: &Path) -> bool {
    dir.join(HISTORY_SNAPSHOT_FILE).is_file()
}

/// Replays the old snapshot and journal once, publishes the result as a stream,
/// reads it back, and only then removes the old files. A failure before the
/// stream head is published leaves the old files authoritative.
///
/// The caller must first make the Task's Subagent catalog reject an older App
/// Server, which would otherwise find the old files gone and mark the history lost.
pub(super) fn migrate(
    task_dir: &Path,
    dir: &Path,
) -> Result<(SubagentChat, StreamFiles), RuntimeError> {
    let started_at = std::time::Instant::now();
    let (history, frame_count) = load_history(dir)?;
    let mut chat = SubagentChat {
        revision: history.revision,
        messages: history.messages,
        message_meta: history.message_meta,
        current_plan: history.current_plan,
    };
    // Histories written before ADR-0075 hold inline images; a stream holds only
    // references to the Task's image files.
    let moved = crate::storage::chat_images::externalize_messages(task_dir, &mut chat.messages)?;
    let described =
        crate::storage::chat_images::add_missing_dimensions(task_dir, &mut chat.messages);
    stream::publish_generation(dir, &chat)?;
    let (verified, files) = stream::load(dir)?;
    if serde_json::to_value(&verified)? != serde_json::to_value(&chat)? {
        return Err(RuntimeError::Storage(
            "Migrated Subagent stream failed validation".to_string(),
        ));
    }
    let removed = [HISTORY_JOURNAL_FILE, HISTORY_SNAPSHOT_FILE]
        .into_iter()
        .try_for_each(|name| match fs::remove_file(dir.join(name)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        });
    if let Err(error) = removed {
        // The stream head is authoritative; leftover old files only take space.
        crate::logging::warn(
            "subagent_legacy_history_cleanup_failed",
            serde_json::json!({
                "subagent_id": dir.file_name().map(|name| name.to_string_lossy()),
                "error_kind": format!("{:?}", error.kind()),
            }),
        );
    }
    crate::logging::info(
        "subagent_history_migrated",
        serde_json::json!({
            "task_id": task_dir.file_name().map(|name| name.to_string_lossy()),
            "subagent_id": dir.file_name().map(|name| name.to_string_lossy()),
            "journal_frame_count": frame_count,
            "moved_image_count": moved,
            "described_image_count": described,
            "message_count": chat.messages.len(),
            "duration_ms": started_at.elapsed().as_millis(),
        }),
    );
    Ok((chat, files))
}

/// Returns the history and how many journal frames its replay read.
fn load_history(dir: &Path) -> Result<(HistoryFile, u64), RuntimeError> {
    let bytes = fs::read(dir.join(HISTORY_SNAPSHOT_FILE))?;
    let mut history: HistoryFile = serde_json::from_slice(&bytes)?;
    validate_history_schema(history.schema_version)?;
    let snapshot_revision = history.revision;
    let journal = dir.join(HISTORY_JOURNAL_FILE);
    let file = fs::File::open(&journal)?;
    let mut reader = BufReader::new(file);
    let mut complete_bytes = 0;
    let mut frame_count = 0;
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if !line.ends_with(b"\n") {
            // A frame is published only with its newline. Discard an interrupted
            // final write; completed malformed frames fail.
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
        frame_count += 1;
        let frame: HistoryFrame = serde_json::from_slice(&line)?;
        let revision = frame.revision();
        if revision <= snapshot_revision {
            // Snapshot publication preceded journal replacement, so an
            // interrupted compaction may leave an already-materialized prefix.
            continue;
        }
        if revision != history.revision.saturating_add(1) {
            return Err(RuntimeError::Storage(
                "Subagent history journal sequence is invalid".to_string(),
            ));
        }
        match frame {
            HistoryFrame::Full { history: next, .. } => {
                validate_history_schema(next.schema_version)?;
                if revision != next.revision {
                    return Err(RuntimeError::Storage(
                        "Subagent history frame revision is inconsistent".to_string(),
                    ));
                }
                history = next;
                continue;
            }
            HistoryFrame::AppendText {
                append_text,
                message_meta,
                ..
            } => {
                append_text.apply(&mut history)?;
                history.message_meta = message_meta;
            }
            HistoryFrame::Upsert {
                upsert,
                message_meta,
                ..
            } => {
                match history
                    .messages
                    .iter_mut()
                    .find(|stored| stored.chat.identity == upsert.chat.identity)
                {
                    Some(stored) => *stored = *upsert,
                    None => history.messages.push(*upsert),
                }
                history.message_meta = message_meta;
            }
            HistoryFrame::SetPlan { set_plan, .. } => history.current_plan = set_plan.plan,
        }
        history.revision = revision;
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
    Ok((history, frame_count))
}

fn validate_history_schema(version: u16) -> Result<(), RuntimeError> {
    if (1..=HISTORY_SCHEMA_VERSION).contains(&version) {
        Ok(())
    } else {
        Err(RuntimeError::Storage(
            "Unsupported Subagent history schema".to_string(),
        ))
    }
}
