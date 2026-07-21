use std::collections::HashSet;

use super::ListingOrderValidator;
use crate::protocol::model::AgentListedSession;

#[test]
fn listing_order_accepts_descending_pages_and_equal_timestamps() {
    let mut validator = ListingOrderValidator::new();
    let mut seen = HashSet::new();

    let first = validator.observe(
        &[
            session("new", Some("2026-07-03T00:00:00Z")),
            session("equal-a", Some("2026-07-02T00:00:00Z")),
            session("equal-b", Some("2026-07-02T00:00:00Z")),
        ],
        &mut seen,
    );
    validator.observe(
        &[
            session("equal-b", Some("2026-07-04T00:00:00Z")),
            session("old", Some("2026-07-01T00:00:00Z")),
        ],
        &mut seen,
    );

    assert!(validator.trusted);
    assert_eq!(first.new_identity_count, 3);
    assert_eq!(
        first.activity_frontier,
        crate::time::activity_millis("2026-07-02T00:00:00Z"),
    );
}

#[test]
fn listing_order_rejects_missing_invalid_and_ascending_activity() {
    for sessions in [
        vec![session("missing", None)],
        vec![session("invalid", Some("yesterday-ish"))],
        vec![
            session("old", Some("2026-07-01T00:00:00Z")),
            session("new", Some("2026-07-02T00:00:00Z")),
        ],
    ] {
        let mut validator = ListingOrderValidator::new();
        validator.observe(&sessions, &mut HashSet::new());
        assert!(!validator.trusted);
    }
}

#[test]
fn listing_order_rejects_a_later_page_that_crosses_the_prior_frontier() {
    let mut validator = ListingOrderValidator::new();
    let mut seen = HashSet::new();
    validator.observe(
        &[
            session("first-new", Some("2026-07-03T00:00:00Z")),
            session("first-old", Some("2026-07-02T00:00:00Z")),
        ],
        &mut seen,
    );
    validator.observe(
        &[session("crossing", Some("2026-07-02T12:00:00Z"))],
        &mut seen,
    );

    assert!(!validator.trusted);
}

fn session(id: &str, activity: Option<&str>) -> AgentListedSession {
    AgentListedSession {
        session_id: id.to_string(),
        cwd: "/workspace".to_string(),
        title: None,
        last_activity: activity.map(str::to_string),
        updated_at: None,
    }
}
