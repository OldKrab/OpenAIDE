//! Server-side search over Task Navigation rows (ADR-0086, "Search").

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{MessageId, ProjectId};
use crate::snapshot::TaskNavigationEntry;

use super::TaskNavigationSection;

/// One-shot search request. `text` is a literal, case-insensitive phrase.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskNavigationSearchParams {
    pub text: String,
    /// Restricts the search to these Projects; absent means every Project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<ProjectId>>,
}

/// A half-open `[start, end)` range of UTF-16 code units, so a JavaScript client can
/// slice the string directly.
pub type TextHighlight = (u32, u32);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchSnippet {
    /// A single-line window of the matching Chat message; a leading or trailing
    /// ellipsis marks a cut window.
    pub text: String,
    #[ts(type = "Array<[number, number]>")]
    pub highlights: Vec<TextHighlight>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskNavigationSearchHit {
    pub row: TaskNavigationEntry,
    /// Which Navigation section the row belongs to.
    pub section: TaskNavigationSection,
    /// Ranges of the row's title that match, when the title matches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number]>")]
    pub title_highlights: Vec<TextHighlight>,
    /// Absent for a title-only match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<SearchSnippet>,
    /// The Chat message the snippet came from; present exactly when `snippet` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskNavigationSearchResult {
    pub hits: Vec<TaskNavigationSearchHit>,
    /// True when the result cap or the scan budget cut the search short, so more rows
    /// could match.
    pub truncated: bool,
}
