//! One-shot search over Task Navigation rows (ADR-0086, "Search").
//!
//! Rows come from the Navigation source the Navigation views read (membership filters
//! included), for both the open and the archived section, and a hit carries the row body
//! a Navigation frame would. Matching is a case-insensitive literal phrase:
//!
//! - titles of Tasks and Native Sessions;
//! - the Chat text of OpenAIDE Tasks: User messages and Agent answers (not thoughts,
//!   Tool output or plans), read in place from the Task's projection of `chat.log`.
//!
//! Ranking: rows whose title matches come first, then rows that match only in Chat text;
//! each group is ordered by recency (last activity, newest first), then by identity.
//! A row's snippet is cut from the first matching message in Chat order. Work is bounded
//! by `MAX_HITS` rows and `MAX_SCANNED_BYTES` of Chat text per request; a request that
//! hits either bound says so with `truncated`. Native Sessions match on title only, and
//! only those already in the Native Session catalog.

mod matcher;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{MessageId, ProjectId};
use openaide_app_server_protocol::navigation::NavigationRow;
use openaide_app_server_protocol::snapshot::TaskNavigationEntry;
use openaide_app_server_protocol::task::{
    SearchSnippet, TaskNavigationSearchHit, TaskNavigationSearchParams, TaskNavigationSearchResult,
    TaskNavigationSection, TextHighlight,
};

use crate::navigation::{live_by_state, NavigationSource, SourceGroup};
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, NormalizedMessage};
use crate::storage::Store;

use matcher::{Matcher, Scratch};

pub(crate) const MAX_HITS: usize = 50;
/// Characters; a longer phrase is refused rather than silently cut.
pub(crate) const MAX_QUERY_CHARS: usize = 256;
/// Bytes of Chat text examined per request.
pub(crate) const MAX_SCANNED_BYTES: usize = 32 * 1024 * 1024;

pub(crate) trait NavigationSearchWorkflow: Send + Sync {
    fn search(
        &self,
        params: TaskNavigationSearchParams,
    ) -> Result<TaskNavigationSearchResult, ProtocolError>;
}

/// Matches nothing; the default for a gateway built without a search backend.
pub(crate) struct NoNavigationSearch;

impl NavigationSearchWorkflow for NoNavigationSearch {
    fn search(
        &self,
        _params: TaskNavigationSearchParams,
    ) -> Result<TaskNavigationSearchResult, ProtocolError> {
        Ok(TaskNavigationSearchResult {
            hits: Vec::new(),
            truncated: false,
        })
    }
}

pub(crate) struct NavigationSearch {
    navigation: Arc<dyn NavigationSource>,
    store: Store,
    limits: Limits,
}

#[derive(Clone, Copy)]
struct Limits {
    max_hits: usize,
    max_scanned_bytes: usize,
}

impl NavigationSearch {
    pub(crate) fn new(navigation: Arc<dyn NavigationSource>, store: Store) -> Self {
        Self {
            navigation,
            store,
            limits: Limits {
                max_hits: MAX_HITS,
                max_scanned_bytes: MAX_SCANNED_BYTES,
            },
        }
    }

    #[cfg(test)]
    fn with_limits(mut self, max_hits: usize, max_scanned_bytes: usize) -> Self {
        self.limits = Limits {
            max_hits,
            max_scanned_bytes,
        };
        self
    }
}

/// A row that matched, before ranking.
struct Candidate {
    row: TaskNavigationEntry,
    section: TaskNavigationSection,
    title_highlights: Vec<TextHighlight>,
    activity: Option<i128>,
}

#[derive(Default)]
struct ScanStats {
    tasks_scanned: usize,
    bytes_scanned: usize,
    unreadable_tasks: usize,
    budget_exhausted: bool,
}

impl NavigationSearchWorkflow for NavigationSearch {
    fn search(
        &self,
        params: TaskNavigationSearchParams,
    ) -> Result<TaskNavigationSearchResult, ProtocolError> {
        let started = Instant::now();
        let text_chars = params.text.chars().count();
        crate::logging::info(
            "navigation_search_started",
            serde_json::json!({
                "operation": "taskNavigation/search",
                "text_length": text_chars,
                "project_filter_count": params.project_ids.as_ref().map(Vec::len),
                "attempt": 1,
            }),
        );
        if text_chars > MAX_QUERY_CHARS {
            return Err(finish_failed(
                started,
                ProtocolErrorCode::InvalidRequest,
                "Search text is too long.",
                false,
            ));
        }
        let Some(matcher) = Matcher::new(&params.text) else {
            log_finished(started, "empty_query", 0, 0, &ScanStats::default(), false);
            return Ok(TaskNavigationSearchResult {
                hits: Vec::new(),
                truncated: false,
            });
        };
        let project_ids: Option<HashSet<ProjectId>> =
            params.project_ids.map(|ids| ids.into_iter().collect());
        let mut rows = Vec::new();
        for section in [TaskNavigationSection::Tasks, TaskNavigationSection::Archive] {
            let groups = self
                .navigation
                .groups(section, project_ids.as_ref())
                .map_err(|error| {
                    finish_failed(
                        started,
                        error.code,
                        "Failed to read Task Navigation.",
                        error.recoverable,
                    )
                })?;
            collect_rows(groups, section, &mut rows);
        }
        let considered = rows.len();
        let mut stats = ScanStats::default();
        let (hits, truncated) = self.rank_and_scan(&matcher, rows, &mut stats);
        log_finished(started, "ok", considered, hits.len(), &stats, truncated);
        Ok(TaskNavigationSearchResult { hits, truncated })
    }
}

impl NavigationSearch {
    fn rank_and_scan(
        &self,
        matcher: &Matcher,
        rows: Vec<(TaskNavigationEntry, TaskNavigationSection)>,
        stats: &mut ScanStats,
    ) -> (Vec<TaskNavigationSearchHit>, bool) {
        let mut titled = Vec::new();
        let mut untitled = Vec::new();
        for (row, section) in rows {
            let highlights = row_title(&row)
                .map(|title| matcher.title_highlights(title))
                .unwrap_or_default();
            let candidate = Candidate {
                activity: row_activity(&row),
                row,
                section,
                title_highlights: highlights,
            };
            if !candidate.title_highlights.is_empty() {
                titled.push(candidate);
            } else if matches!(candidate.row, TaskNavigationEntry::Task { .. }) {
                // Only OpenAIDE Tasks have Chat text to match.
                untitled.push(candidate);
            }
        }
        titled.sort_by(order);
        untitled.sort_by(order);
        let limit = self.limits.max_hits;
        let mut truncated = titled.len() > limit;
        titled.truncate(limit);
        let mut scratch = Scratch::default();
        let mut hits = Vec::new();
        for candidate in titled {
            let found = self.first_match(matcher, &candidate.row, &mut scratch, stats);
            hits.push(into_hit(candidate, found));
        }
        for candidate in untitled {
            if hits.len() >= limit {
                truncated = true;
                break;
            }
            if let Some(found) = self.first_match(matcher, &candidate.row, &mut scratch, stats) {
                hits.push(into_hit(candidate, Some(found)));
            }
        }
        (hits, truncated || stats.budget_exhausted)
    }

    /// The first matching message of a Task's Chat, within the request's scan budget.
    fn first_match(
        &self,
        matcher: &Matcher,
        row: &TaskNavigationEntry,
        scratch: &mut Scratch,
        stats: &mut ScanStats,
    ) -> Option<(MessageId, SearchSnippet)> {
        let TaskNavigationEntry::Task { task } = row else {
            return None;
        };
        if stats.bytes_scanned >= self.limits.max_scanned_bytes {
            stats.budget_exhausted = true;
            return None;
        }
        let scanned = self
            .store
            .inspect_messages(task.task_id.as_str(), |messages| {
                let mut bytes = 0usize;
                for stored in messages {
                    for text in message_texts(&stored.chat.message) {
                        bytes += text.len();
                        if let Some(snippet) = matcher.snippet(text, scratch) {
                            let id = MessageId::from(stored.chat.message_id.clone());
                            return (bytes, Some((id, snippet)));
                        }
                    }
                }
                (bytes, None)
            });
        match scanned {
            Ok((bytes, found)) => {
                stats.tasks_scanned += 1;
                stats.bytes_scanned += bytes;
                found
            }
            Err(_) => {
                // The Task stays reachable through its title; the failure is counted, and
                // the Store already logged its own cause.
                stats.unreadable_tasks += 1;
                None
            }
        }
    }
}

fn into_hit(
    candidate: Candidate,
    found: Option<(MessageId, SearchSnippet)>,
) -> TaskNavigationSearchHit {
    let (message_id, snippet) = match found {
        Some((id, snippet)) => (Some(id), Some(snippet)),
        None => (None, None),
    };
    TaskNavigationSearchHit {
        row: NavigationRow {
            live: live_by_state(&candidate.row),
            entry: candidate.row,
        },
        section: candidate.section,
        title_highlights: candidate.title_highlights,
        snippet,
        message_id,
    }
}

fn order(left: &Candidate, right: &Candidate) -> std::cmp::Ordering {
    right
        .activity
        .cmp(&left.activity)
        .then_with(|| identity(&left.row).cmp(&identity(&right.row)))
}

fn identity(row: &TaskNavigationEntry) -> (&str, &str) {
    match row {
        TaskNavigationEntry::Task { task } => (task.agent_id.as_str(), task.task_id.as_str()),
        TaskNavigationEntry::NativeSession { session } => (
            session.reference.agent_id.as_str(),
            session.reference.session_id.as_str(),
        ),
    }
}

fn row_title(row: &TaskNavigationEntry) -> Option<&str> {
    match row {
        TaskNavigationEntry::Task { task } => task.title.as_ref().map(|title| title.value.as_str()),
        TaskNavigationEntry::NativeSession { session } => session.title.as_deref(),
    }
}

fn row_activity(row: &TaskNavigationEntry) -> Option<i128> {
    match row {
        TaskNavigationEntry::Task { task } => crate::time::activity_millis(&task.last_activity),
        TaskNavigationEntry::NativeSession { session } => session
            .last_activity
            .as_deref()
            .and_then(crate::time::activity_millis),
    }
}

fn collect_rows(
    groups: Vec<SourceGroup>,
    section: TaskNavigationSection,
    rows: &mut Vec<(TaskNavigationEntry, TaskNavigationSection)>,
) {
    rows.extend(
        groups
            .into_iter()
            .flat_map(|group| group.entries)
            .map(|entry| (entry, section)),
    );
}

/// The searchable text parts of one Chat message.
fn message_texts(message: &NormalizedMessage) -> Vec<&str> {
    match message {
        NormalizedMessage::User { text, .. } => vec![text.as_str()],
        NormalizedMessage::AgentMessage {
            role: AgentMessageRole::Agent,
            parts,
            ..
        } => parts
            .iter()
            .filter_map(|part| match part {
                AgentMessagePart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn log_finished(
    started: Instant,
    outcome: &str,
    rows_considered: usize,
    hits: usize,
    stats: &ScanStats,
    truncated: bool,
) {
    crate::logging::info(
        "navigation_search_finished",
        serde_json::json!({
            "operation": "taskNavigation/search",
            "outcome": outcome,
            "rows_considered": rows_considered,
            "hit_count": hits,
            "tasks_scanned": stats.tasks_scanned,
            "bytes_scanned": stats.bytes_scanned,
            "unreadable_tasks": stats.unreadable_tasks,
            "budget_exhausted": stats.budget_exhausted,
            "truncated": truncated,
            "duration_ms": started.elapsed().as_millis(),
            "attempt": 1,
        }),
    );
}

fn finish_failed(
    started: Instant,
    code: ProtocolErrorCode,
    message: &str,
    recoverable: bool,
) -> ProtocolError {
    crate::logging::warn(
        "navigation_search_finished",
        serde_json::json!({
            "operation": "taskNavigation/search",
            "outcome": "failed",
            "error_kind": serde_json::to_value(code).ok(),
            "duration_ms": started.elapsed().as_millis(),
            "attempt": 1,
        }),
    );
    ProtocolError {
        code,
        message: message.to_string(),
        recoverable,
        target: None,
    }
}
