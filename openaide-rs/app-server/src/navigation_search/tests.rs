use std::sync::Arc;
use std::time::Instant;

use openaide_app_server_protocol::snapshot::TaskNavigationEntry;
use openaide_app_server_protocol::task::{
    TaskNavigationSearchParams, TaskNavigationSearchResult, TaskNavigationSection,
};

use super::matcher::{Matcher, Scratch};
use super::*;
use crate::native_sessions::catalog::{
    NativeSessionCatalog, NativeSessionObservation, NativeSessionRef,
};
use crate::protocol::model::{
    AgentMessagePart, AgentMessageRole, ChatMessage, IsolationKind, NormalizedMessage, TaskStatus,
};
use crate::snapshots::TaskNavigationStore;
use crate::storage::records::{TaskLifecycle, TaskRecord};

fn snippet(query: &str, text: &str) -> Option<SearchSnippet> {
    Matcher::new(query)?.snippet(text, &mut Scratch::default())
}

fn highlighted(snippet: &SearchSnippet) -> Vec<String> {
    let units: Vec<u16> = snippet.text.encode_utf16().collect();
    snippet
        .highlights
        .iter()
        .map(|&(start, end)| String::from_utf16(&units[start as usize..end as usize]).unwrap())
        .collect()
}

#[test]
fn matching_ignores_case_and_highlights_every_occurrence() {
    let found = snippet("rust", "Rust is nice. I like RUST and rust.").unwrap();

    assert_eq!(found.text, "Rust is nice. I like RUST and rust.");
    assert_eq!(found.highlights, vec![(0, 4), (21, 25), (30, 34)]);
    assert_eq!(highlighted(&found), vec!["Rust", "RUST", "rust"]);
}

#[test]
fn blank_query_matches_nothing_and_text_without_it_has_no_snippet() {
    assert!(Matcher::new("   ").is_none());
    assert!(snippet("zebra", "nothing to see").is_none());
}

#[test]
fn long_text_is_windowed_around_the_first_match_with_ellipses() {
    let text = format!("{} needle {}", "lead ".repeat(60), "tail ".repeat(60));

    let found = snippet("needle", &text).unwrap();

    assert!(found.text.starts_with('\u{2026}'));
    assert!(found.text.ends_with('\u{2026}'));
    assert!(found.text.chars().count() < 200);
    assert_eq!(highlighted(&found), vec!["needle"]);
}

#[test]
fn a_match_at_the_start_has_no_leading_ellipsis() {
    let text = format!("needle {}", "x".repeat(500));

    let found = snippet("needle", &text).unwrap();

    assert!(!found.text.starts_with('\u{2026}'));
    assert!(found.text.ends_with('\u{2026}'));
    assert_eq!(found.highlights[0], (0, 6));
}

#[test]
fn newlines_become_spaces_so_the_snippet_is_one_line() {
    let found = snippet("two", "one\ntwo\r\nthree").unwrap();

    assert_eq!(found.text, "one two  three");
}

#[test]
fn ranges_count_utf16_units_for_astral_characters_and_non_ascii_case() {
    // The emoji is two UTF-16 units; the Cyrillic capital folds to its lowercase form.
    let found = snippet("привет", "\u{1F600} ПРИВЕТ").unwrap();

    assert_eq!(found.highlights, vec![(3, 9)]);
    assert_eq!(highlighted(&found), vec!["ПРИВЕТ"]);
}

#[test]
fn title_highlights_cover_all_matches() {
    let matcher = Matcher::new("ab").unwrap();

    assert_eq!(matcher.title_highlights("Ab cab"), vec![(0, 2), (4, 6)]);
    assert!(matcher.title_highlights("none").is_empty());
}

fn task_record(task_id: &str, title: &str, activity: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitleState::from_title(
            crate::storage::records::TaskTitle::new(
                title,
                crate::storage::records::TaskTitleSource::User,
            ),
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        pinned: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: activity.to_string(),
        last_activity: activity.to_string(),
        permission_policy: Default::default(),
        composer_history: Default::default(),
        message_queue: Default::default(),
        agent_id: "agent-a".to_string(),
        agent_name: "Agent A".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace/a".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: TaskLifecycle::Open,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        background_work: None,
        tombstoned: false,
        revision: 1,
        config_options_catalog: None,
        native_session_data_freshness: Default::default(),
        native_session_reload_requirement: None,
        native_session_own_activity_at: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        context_usage: None,
        current_plan: None,
        completed_plan_message_id: None,
        last_turn_usage: None,
        model_id: None,
        supports_image_input: false,
        preparation: crate::storage::records::TaskPreparationRecord::Ready,
    }
}

fn user(id: &str, text: &str) -> ChatMessage {
    ChatMessage {
        cursor: id.to_string(),
        identity: id.to_string(),
        message_type: "user".to_string(),
        message_id: id.to_string(),
        message: NormalizedMessage::User {
            id: id.to_string(),
            text: text.to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            attachments: Vec::new(),
        },
        timing: Default::default(),
    }
}

fn agent(id: &str, role: AgentMessageRole, text: &str) -> ChatMessage {
    ChatMessage {
        cursor: id.to_string(),
        identity: id.to_string(),
        message_type: "agent_message".to_string(),
        message_id: id.to_string(),
        message: NormalizedMessage::AgentMessage {
            id: id.to_string(),
            role,
            parts: vec![AgentMessagePart::Text {
                text: text.to_string(),
            }],
            created_at: "2026-07-01T00:00:00Z".to_string(),
        },
        timing: Default::default(),
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    catalog: NativeSessionCatalog,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let catalog = NativeSessionCatalog::open(store.clone()).unwrap();
        Self {
            _temp: temp,
            store,
            catalog,
        }
    }

    fn task(&self, id: &str, title: &str, activity: &str, messages: &[ChatMessage]) {
        self.task_in(id, title, activity, TaskLifecycle::Open, messages);
    }

    fn task_in(
        &self,
        id: &str,
        title: &str,
        activity: &str,
        lifecycle: TaskLifecycle,
        messages: &[ChatMessage],
    ) {
        let mut record = task_record(id, title, activity);
        record.lifecycle = lifecycle;
        // A Task nobody has written to is not in Navigation, so not searchable either; a titled
        // fixture without stored messages stands for one that has been written to.
        record.message_history_version = messages.len().max(1) as u64;
        self.store.write_task(&record).unwrap();
        for message in messages {
            self.store.append_message(id, message.clone()).unwrap();
        }
    }

    fn search(&self, text: &str) -> TaskNavigationSearchResult {
        self.search_with(text, MAX_HITS, MAX_SCANNED_BYTES)
    }

    fn search_with(&self, text: &str, hits: usize, bytes: usize) -> TaskNavigationSearchResult {
        let navigation = Arc::new(TaskNavigationStore::with_native_sessions(
            self.store.clone(),
            self.catalog.clone(),
        ));
        NavigationSearch::new(navigation, self.store.clone())
            .with_limits(hits, bytes)
            .search(TaskNavigationSearchParams {
                text: text.to_string(),
                project_ids: None,
            })
            .unwrap()
    }
}

fn task_id(hit: &TaskNavigationSearchHit) -> &str {
    match &hit.row.entry {
        TaskNavigationEntry::Task { task } => task.task_id.as_str(),
        TaskNavigationEntry::NativeSession { session } => session.reference.session_id.as_str(),
    }
}

#[test]
fn chat_text_match_returns_snippet_and_message_id() {
    let fixture = Fixture::new();
    fixture.task(
        "task-1",
        "Unrelated",
        "2026-01-02T00:00:00.000Z",
        &[
            user("m1", "hello there"),
            agent(
                "m2",
                AgentMessageRole::Agent,
                "The Quokka lives in Australia.",
            ),
        ],
    );

    let result = fixture.search("quokka");

    assert_eq!(result.hits.len(), 1);
    let hit = &result.hits[0];
    assert_eq!(hit.message_id.as_ref().map(|id| id.as_str()), Some("m2"));
    assert_eq!(hit.section, TaskNavigationSection::Tasks);
    assert!(hit.title_highlights.is_empty());
    assert_eq!(highlighted(hit.snippet.as_ref().unwrap()), vec!["Quokka"]);
    assert!(!result.truncated);
}

#[test]
fn the_first_matching_message_in_chat_order_is_the_snippet() {
    let fixture = Fixture::new();
    fixture.task(
        "task-1",
        "T",
        "2026-01-02T00:00:00.000Z",
        &[user("first", "alpha one"), user("second", "alpha two")],
    );

    let result = fixture.search("alpha");

    assert_eq!(
        result.hits[0].message_id.as_ref().unwrap().as_str(),
        "first"
    );
}

#[test]
fn thoughts_are_not_searched() {
    let fixture = Fixture::new();
    fixture.task(
        "task-1",
        "T",
        "2026-01-02T00:00:00.000Z",
        &[agent("m1", AgentMessageRole::Thought, "secret pondering")],
    );

    assert!(fixture.search("pondering").hits.is_empty());
}

#[test]
fn title_only_match_has_no_snippet_and_highlights_the_title() {
    let fixture = Fixture::new();
    fixture.task(
        "task-1",
        "Fix the Parser",
        "2026-01-02T00:00:00.000Z",
        &[user("m1", "unrelated")],
    );

    let result = fixture.search("parser");

    assert_eq!(result.hits.len(), 1);
    assert!(result.hits[0].snippet.is_none());
    assert!(result.hits[0].message_id.is_none());
    assert_eq!(result.hits[0].title_highlights, vec![(8, 14)]);
}

#[test]
fn title_match_with_chat_text_also_gets_a_snippet() {
    let fixture = Fixture::new();
    fixture.task(
        "task-1",
        "Parser work",
        "2026-01-02T00:00:00.000Z",
        &[user("m1", "the parser fails")],
    );

    let hit = &fixture.search("parser").hits[0];

    assert!(!hit.title_highlights.is_empty());
    assert_eq!(hit.message_id.as_ref().unwrap().as_str(), "m1");
}

#[test]
fn title_matches_rank_before_text_matches_then_recency() {
    let fixture = Fixture::new();
    fixture.task(
        "text-new",
        "x",
        "2026-01-09T00:00:00.000Z",
        &[user("m", "needle")],
    );
    fixture.task("title-old", "needle", "2026-01-01T00:00:00.000Z", &[]);
    fixture.task(
        "text-old",
        "x",
        "2026-01-02T00:00:00.000Z",
        &[user("m", "needle")],
    );
    fixture.task("title-new", "a needle", "2026-01-05T00:00:00.000Z", &[]);

    let order: Vec<_> = fixture
        .search("needle")
        .hits
        .iter()
        .map(|hit| task_id(hit).to_string())
        .collect();

    assert_eq!(order, ["title-new", "title-old", "text-new", "text-old"]);
}

#[test]
fn archived_tasks_are_searched_and_name_their_section() {
    let fixture = Fixture::new();
    fixture.task_in(
        "archived",
        "x",
        "2026-01-02T00:00:00.000Z",
        TaskLifecycle::Archived,
        &[user("m1", "walrus")],
    );

    let result = fixture.search("walrus");

    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].section, TaskNavigationSection::Archive);
}

#[test]
fn native_sessions_match_by_title_in_either_section() {
    let fixture = Fixture::new();
    let open = NativeSessionRef::new("agent-a", "native-open");
    let archived = NativeSessionRef::new("agent-a", "native-archived");
    fixture
        .catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![
                NativeSessionObservation {
                    reference: open,
                    title: Some("Refactor Gizmo".to_string()),
                    last_activity: Some("2026-01-03T00:00:00.000Z".to_string()),
                },
                NativeSessionObservation {
                    reference: archived.clone(),
                    title: Some("Old gizmo notes".to_string()),
                    last_activity: Some("2026-01-02T00:00:00.000Z".to_string()),
                },
            ],
        )
        .unwrap();
    fixture.catalog.archive(&archived).unwrap();

    let result = fixture.search("GIZMO");

    let found: Vec<_> = result
        .hits
        .iter()
        .map(|hit| (task_id(hit).to_string(), hit.section))
        .collect();
    assert_eq!(
        found,
        [
            ("native-open".to_string(), TaskNavigationSection::Tasks),
            (
                "native-archived".to_string(),
                TaskNavigationSection::Archive
            ),
        ]
    );
    assert!(result.hits.iter().all(|hit| hit.snippet.is_none()));
}

#[test]
fn result_count_is_capped_and_marked_truncated() {
    let fixture = Fixture::new();
    for index in 0..5 {
        fixture.task(
            &format!("task-{index}"),
            "match",
            &format!("2026-01-0{}T00:00:00.000Z", index + 1),
            &[],
        );
    }

    let result = fixture.search_with("match", 3, MAX_SCANNED_BYTES);

    assert_eq!(result.hits.len(), 3);
    assert!(result.truncated);
    assert_eq!(task_id(&result.hits[0]), "task-4");
}

#[test]
fn scan_budget_stops_reading_chats_and_marks_truncated() {
    let fixture = Fixture::new();
    for index in 0..3 {
        fixture.task(
            &format!("task-{index}"),
            "x",
            &format!("2026-01-0{}T00:00:00.000Z", index + 1),
            &[user("m", &format!("{} needle", "padding ".repeat(20)))],
        );
    }

    let result = fixture.search_with("needle", MAX_HITS, 100);

    // The newest Task is read; the budget is spent before the rest.
    assert!(result.hits.len() < 3);
    assert!(result.truncated);
}

#[test]
fn too_long_text_is_refused_and_blank_text_is_empty() {
    let fixture = Fixture::new();
    let navigation = Arc::new(TaskNavigationStore::new(fixture.store.clone()));
    let search = NavigationSearch::new(navigation, fixture.store.clone());

    let long = search.search(TaskNavigationSearchParams {
        text: "a".repeat(MAX_QUERY_CHARS + 1),
        project_ids: None,
    });
    let blank = search
        .search(TaskNavigationSearchParams {
            text: "  ".to_string(),
            project_ids: None,
        })
        .unwrap();

    assert!(long.is_err());
    assert!(blank.hits.is_empty());
}

/// Measurement, not a gate: scanning the whole budget of Chat text without a stored index.
#[test]
fn scanning_the_full_budget_stays_interactive() {
    let matcher = Matcher::new("zzzzzz-no-such-phrase").unwrap();
    let paragraph = "The quick brown fox jumps over the lazy dog while reading logs. ".repeat(16);
    let mut scratch = Scratch::default();
    let started = Instant::now();
    let mut scanned = 0;
    while scanned < MAX_SCANNED_BYTES {
        assert!(matcher.snippet(&paragraph, &mut scratch).is_none());
        scanned += paragraph.len();
    }
    let elapsed = started.elapsed();
    eprintln!("scanned {scanned} bytes in {elapsed:?}");
    assert!(elapsed.as_secs() < 20, "scan took {elapsed:?}");
}
