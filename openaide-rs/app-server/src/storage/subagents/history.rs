use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, AgentPlan, NormalizedMessage};
use crate::storage::records::{MessageMeta, StoredMessage};

pub(super) const HISTORY_SNAPSHOT_FILE: &str = "history.snapshot";
pub(super) const HISTORY_JOURNAL_FILE: &str = "history.journal";
/// Bounds replay work; the byte budget below bounds replay size.
pub(super) const COMPACT_AFTER_FRAMES: u64 = 256;
/// Schema 2 journals may hold message and plan frames. Schema 1 journals hold only
/// full-history and text frames; an App Server that knows only schema 1 rejects a
/// schema 2 history instead of misreading frames it does not know.
pub(super) const HISTORY_SCHEMA_VERSION: u16 = 2;
/// The smallest journal worth keeping beside a snapshot before compacting.
const MIN_JOURNAL_BUDGET_BYTES: u64 = 1 << 20;

/// How many journal bytes a snapshot of this size may carry. Compacting only once
/// the journal would outgrow its snapshot keeps the rewritten bytes proportional
/// to the change bytes, and a load reads at most about twice the snapshot.
pub(super) fn journal_budget(snapshot_bytes: u64) -> u64 {
    snapshot_bytes.max(MIN_JOURNAL_BUDGET_BYTES)
}

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
            schema_version: HISTORY_SCHEMA_VERSION,
            revision: 0,
            journal_frames: 0,
            messages: Vec::new(),
            message_meta: MessageMeta::default(),
            current_plan: None,
        }
    }
}

// Full frames are the original schema-1 journal representation and remain readable;
// current writers journal only what changed. Streamed Agent text uses a semantic
// append, any other message change carries that one message, and a plan change
// carries the plan. Metadata is committed in the same newline-delimited frame.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
enum HistoryFrame<H, M, P> {
    Full {
        revision: u64,
        history: H,
    },
    AppendText {
        revision: u64,
        append_text: TextAppend,
        message_meta: MessageMeta,
    },
    Upsert {
        revision: u64,
        upsert: M,
        message_meta: MessageMeta,
    },
    SetPlan {
        revision: u64,
        set_plan: PlanChange<P>,
    },
}

/// A required wrapper, so a frame without `setPlan` cannot read as a cleared plan.
#[derive(Debug, Serialize, Deserialize)]
struct PlanChange<P> {
    plan: Option<P>,
}

type StoredFrame = HistoryFrame<HistoryFile, StoredMessage, AgentPlan>;

impl<H, M, P> HistoryFrame<H, M, P> {
    fn revision(&self) -> u64 {
        match self {
            Self::Full { revision, .. }
            | Self::AppendText { revision, .. }
            | Self::Upsert { revision, .. }
            | Self::SetPlan { revision, .. } => *revision,
        }
    }
}

/// What one history mutation changed, so the journal records only that.
pub(super) enum HistoryChange {
    /// Streamed Agent text appended to the identified message.
    AppendText(TextAppend),
    /// The message with this identity was added or replaced.
    Upsert(String),
    /// The current plan was replaced or cleared.
    Plan,
}

impl HistoryChange {
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::AppendText(_) => "append_text",
            Self::Upsert(_) => "upsert",
            Self::Plan => "plan",
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
    validate_history_schema(history.schema_version)?;
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
        let frame: StoredFrame = serde_json::from_slice(&line)?;
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
                validate_history_schema(next.schema_version)?;
                if revision != next.revision {
                    return Err(RuntimeError::Storage(
                        "Subagent history frame revision is inconsistent".to_string(),
                    ));
                }
                history = next;
                repeated_frames += 1;
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
                    Some(stored) => *stored = upsert,
                    None => history.messages.push(upsert),
                }
                history.message_meta = message_meta;
            }
            HistoryFrame::SetPlan { set_plan, .. } => history.current_plan = set_plan.plan,
        }
        history.revision = revision;
        history.journal_frames = history.journal_frames.saturating_add(1);
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

/// Encodes the frame for a change already applied to `history`, newline included.
/// `None` means the change has no frame of its own and the snapshot is published.
pub(super) fn encode_change_frame(
    history: &HistoryFile,
    change: HistoryChange,
) -> Result<Option<Vec<u8>>, RuntimeError> {
    let revision = history.revision;
    let message_meta = history.message_meta.clone();
    let frame = match change {
        HistoryChange::AppendText(append_text) => HistoryFrame::AppendText {
            revision,
            append_text,
            message_meta,
        },
        HistoryChange::Upsert(identity) => {
            let Some(upsert) = history
                .messages
                .iter()
                .find(|stored| stored.chat.identity == identity)
            else {
                return Ok(None);
            };
            HistoryFrame::Upsert {
                revision,
                upsert,
                message_meta,
            }
        }
        HistoryChange::Plan => HistoryFrame::SetPlan {
            revision,
            set_plan: PlanChange {
                plan: history.current_plan.as_ref(),
            },
        },
    };
    let frame: HistoryFrame<(), &StoredMessage, &AgentPlan> = frame;
    let mut bytes = serde_json::to_vec(&frame)?;
    bytes.push(b'\n');
    Ok(Some(bytes))
}

/// Whether the journal can take `frame_bytes` more before it outgrows its snapshot.
pub(super) fn journal_has_room(dir: &Path, frame_bytes: u64) -> Result<bool, RuntimeError> {
    let snapshot = fs::metadata(dir.join(HISTORY_SNAPSHOT_FILE))?.len();
    let journal = fs::metadata(dir.join(HISTORY_JOURNAL_FILE))?.len();
    Ok(journal.saturating_add(frame_bytes) <= journal_budget(snapshot))
}

/// Appends one encoded frame; it is accepted once its newline is durable.
pub(super) fn append_history_frame(path: &Path, frame: &[u8]) -> Result<(), RuntimeError> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(frame)?;
    file.sync_all()?;
    Ok(())
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
