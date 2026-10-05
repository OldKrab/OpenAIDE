//! ACP session compaction projected into App Server Chat state.
//!
//! Live `session/update`s and replayed history share this module so a reloaded
//! Task shows exactly the compaction rows it showed while the Agent was running.

use crate::agent::acp_schema::{
    CompactionStatus as AcpCompactionStatus, CompactionSummaryChunk, CompactionUpdate,
    ContentBlock, MaybeUndefined,
};
use crate::agent::events::AgentCompactionChange;
use crate::agent::AgentMetadataField;
use crate::protocol::model::{CompactionStatus, NormalizedMessage};

/// Returns the Agent-owned compaction id with its normalized change.
pub(crate) fn project_compaction_update(
    update: CompactionUpdate,
) -> (String, AgentCompactionChange) {
    let status = match update.status {
        AcpCompactionStatus::InProgress => CompactionStatus::InProgress,
        AcpCompactionStatus::Completed => CompactionStatus::Completed,
        AcpCompactionStatus::Failed => CompactionStatus::Failed,
        AcpCompactionStatus::Cancelled => CompactionStatus::Cancelled,
        _ => CompactionStatus::Unknown,
    };
    let summary = match update.summary {
        MaybeUndefined::Undefined => AgentMetadataField::Unchanged,
        MaybeUndefined::Null => AgentMetadataField::Clear,
        // An empty array clears the retained summary, as does a summary with
        // no displayable text.
        MaybeUndefined::Value(blocks) => match summary_text(blocks.iter()) {
            Some(text) => AgentMetadataField::Value(text),
            None => AgentMetadataField::Clear,
        },
    };
    let error = match update.error {
        MaybeUndefined::Undefined => AgentMetadataField::Unchanged,
        MaybeUndefined::Null => AgentMetadataField::Clear,
        MaybeUndefined::Value(error) if error.trim().is_empty() => AgentMetadataField::Clear,
        MaybeUndefined::Value(error) => AgentMetadataField::Value(error),
    };
    (
        update.compaction_id.to_string(),
        AgentCompactionChange::Update {
            status,
            summary,
            error,
        },
    )
}

/// Chunks without displayable text return `None`; they leave the summary unchanged.
pub(crate) fn project_compaction_summary_chunk(
    chunk: CompactionSummaryChunk,
) -> Option<(String, AgentCompactionChange)> {
    let text = summary_text(std::iter::once(&chunk.content))?;
    Some((
        chunk.compaction_id.to_string(),
        AgentCompactionChange::SummaryChunk { text },
    ))
}

/// Summary blocks are fragments of one Markdown document: chunk boundaries do
/// not imply paragraph breaks, so text blocks concatenate without a separator.
/// Non-text blocks have no summary representation and are skipped.
fn summary_text<'a>(blocks: impl Iterator<Item = &'a ContentBlock>) -> Option<String> {
    let text: String = blocks
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    (!text.trim().is_empty()).then_some(text)
}

/// Applies one ordered change to the stored compaction, creating it when the
/// Agent has not yet announced the id. A chunk that precedes its `in_progress`
/// update still lands in an in-progress row instead of being dropped.
pub(crate) fn apply_compaction_change(
    id: &str,
    existing: Option<&NormalizedMessage>,
    change: AgentCompactionChange,
    created_at: &str,
) -> NormalizedMessage {
    let (mut status, mut summary, mut error, created_at) = match existing {
        Some(NormalizedMessage::Compaction {
            status,
            summary,
            error,
            created_at,
            ..
        }) => (*status, summary.clone(), error.clone(), created_at.clone()),
        _ => (
            CompactionStatus::InProgress,
            None,
            None,
            created_at.to_string(),
        ),
    };
    match change {
        AgentCompactionChange::Update {
            status: next_status,
            summary: summary_patch,
            error: error_patch,
        } => {
            status = next_status;
            apply_patch(&mut summary, summary_patch);
            apply_patch(&mut error, error_patch);
        }
        AgentCompactionChange::SummaryChunk { text } => {
            summary.get_or_insert_with(String::new).push_str(&text);
        }
    }
    NormalizedMessage::Compaction {
        id: id.to_string(),
        status,
        summary,
        error,
        created_at,
    }
}

fn apply_patch(field: &mut Option<String>, patch: AgentMetadataField<String>) {
    match patch {
        AgentMetadataField::Unchanged => {}
        AgentMetadataField::Clear => *field = None,
        AgentMetadataField::Value(value) => *field = Some(value),
    }
}
