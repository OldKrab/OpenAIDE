use openaide_app_server_protocol::events::TaskChanges;
use openaide_app_server_protocol::ids::TaskId;

use super::{ChangesSince, TaskChangeLog};

fn change(size: usize) -> TaskChanges {
    TaskChanges {
        active_turn_started_at: Some(Some("x".repeat(size))),
        ..TaskChanges::default()
    }
}

fn revisions(since: ChangesSince) -> Vec<u64> {
    match since {
        ChangesSince::Changes { changes, .. } => {
            changes.iter().map(|change| change.revision).collect()
        }
        other => panic!("expected retained changes, got {other:?}"),
    }
}

fn none() -> ChangesSince {
    ChangesSince::Changes {
        changes: Vec::new(),
        bytes: 0,
    }
}

fn task(id: &str) -> TaskId {
    TaskId::from(id.to_string())
}

#[test]
fn replays_only_the_changes_after_the_known_revision() {
    let mut log = TaskChangeLog::default();
    for revision in 4..=7 {
        log.record(&task("a"), revision, &change(10));
    }

    assert_eq!(revisions(log.changes_since(&task("a"), 5)), vec![6, 7]);
    assert_eq!(
        revisions(log.changes_since(&task("a"), 3)),
        vec![4, 5, 6, 7]
    );
}

#[test]
fn a_current_or_ahead_of_publication_copy_misses_nothing() {
    let mut log = TaskChangeLog::default();
    log.record(&task("a"), 7, &change(10));

    assert_eq!(log.changes_since(&task("a"), 7), none());
    assert_eq!(log.changes_since(&task("a"), 9), none());
}

#[test]
fn a_task_this_process_never_published_is_unknown() {
    let log = TaskChangeLog::default();

    assert_eq!(log.changes_since(&task("a"), 3), ChangesSince::Unknown);
}

#[test]
fn a_revision_older_than_the_retained_run_is_evicted() {
    let mut log = TaskChangeLog::default();
    for revision in 4..=7 {
        log.record(&task("a"), revision, &change(10));
    }

    assert_eq!(log.changes_since(&task("a"), 2), ChangesSince::Evicted);
}

#[test]
fn a_publication_gap_restarts_the_retained_run() {
    let mut log = TaskChangeLog::default();
    log.record(&task("a"), 4, &change(10));
    log.record(&task("a"), 5, &change(10));
    log.record(&task("a"), 8, &change(10));

    assert_eq!(log.changes_since(&task("a"), 4), ChangesSince::Evicted);
    assert_eq!(revisions(log.changes_since(&task("a"), 7)), vec![8]);
}

#[test]
fn the_per_task_bound_drops_the_oldest_changes_first() {
    let size = serde_json::to_vec(&change(100)).unwrap().len();
    let mut log = TaskChangeLog::with_bounds(size * 2, usize::MAX);
    for revision in 1..=3 {
        log.record(&task("a"), revision, &change(100));
    }

    assert_eq!(revisions(log.changes_since(&task("a"), 1)), vec![2, 3]);
    assert_eq!(log.changes_since(&task("a"), 0), ChangesSince::Evicted);
    assert_eq!(log.total_bytes(), size * 2);
}

#[test]
fn an_oversized_change_is_never_retained() {
    let size = serde_json::to_vec(&change(100)).unwrap().len();
    let mut log = TaskChangeLog::with_bounds(size, usize::MAX);
    log.record(&task("a"), 1, &change(100));
    log.record(&task("a"), 2, &change(1_000));

    assert_eq!(log.changes_since(&task("a"), 1), ChangesSince::Evicted);
    assert_eq!(log.changes_since(&task("a"), 2), none());
    assert_eq!(log.total_bytes(), 0);
}

#[test]
fn the_total_bound_evicts_the_least_recently_published_task() {
    let size = serde_json::to_vec(&change(100)).unwrap().len();
    let mut log = TaskChangeLog::with_bounds(usize::MAX, size * 2);
    log.record(&task("a"), 1, &change(100));
    log.record(&task("b"), 1, &change(100));
    log.record(&task("c"), 1, &change(100));

    assert_eq!(log.changes_since(&task("a"), 0), ChangesSince::Evicted);
    assert_eq!(log.changes_since(&task("a"), 1), none());
    assert_eq!(revisions(log.changes_since(&task("b"), 0)), vec![1]);
    assert_eq!(revisions(log.changes_since(&task("c"), 0)), vec![1]);
    assert_eq!(log.total_bytes(), size * 2);
}

#[test]
fn a_removed_task_is_forgotten() {
    let mut log = TaskChangeLog::default();
    log.record(&task("a"), 1, &change(10));
    log.record(
        &task("a"),
        2,
        &TaskChanges {
            removed: true,
            ..TaskChanges::default()
        },
    );

    assert_eq!(log.changes_since(&task("a"), 0), ChangesSince::Unknown);
    assert_eq!(log.total_bytes(), 0);
}
