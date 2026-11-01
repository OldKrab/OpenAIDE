use super::*;

fn wait_at(backoff: &EarlyExitBackoff, now: Instant) -> Option<Duration> {
    backoff.held_back("agent", now).map(|held| held.wait)
}

#[test]
fn the_first_exit_may_retry_at_once() {
    let backoff = EarlyExitBackoff::default();
    let now = Instant::now();
    backoff.record_exit("agent", now);
    assert_eq!(wait_at(&backoff, now), None);
}

#[test]
fn repeated_exits_wait_longer_up_to_the_limit() {
    let backoff = EarlyExitBackoff::default();
    let now = Instant::now();
    let waits: Vec<_> = (0..9)
        .map(|_| {
            backoff.record_exit("agent", now);
            wait_at(&backoff, now).map(|wait| wait.as_secs())
        })
        .collect();
    assert_eq!(
        waits,
        [
            None,
            Some(1),
            Some(2),
            Some(4),
            Some(8),
            Some(16),
            Some(30),
            Some(30),
            Some(30)
        ]
    );
}

#[test]
fn the_wait_ends_and_counts_exits_per_agent() {
    let backoff = EarlyExitBackoff::default();
    let now = Instant::now();
    backoff.record_exit("agent", now);
    backoff.record_exit("agent", now);
    assert_eq!(backoff.held_back("agent", now).unwrap().early_exits, 2);
    assert!(backoff.held_back("other", now).is_none());
    assert_eq!(wait_at(&backoff, now + LONGEST_WAIT), None);
}

#[test]
fn clearing_forgets_the_exits() {
    let backoff = EarlyExitBackoff::default();
    let now = Instant::now();
    backoff.record_exit("agent", now);
    backoff.record_exit("agent", now);
    backoff.clear("agent");
    backoff.record_exit("agent", now);
    assert_eq!(wait_at(&backoff, now), None);
}

#[test]
fn a_very_long_streak_stays_at_the_limit() {
    assert_eq!(wait_after(u32::MAX), LONGEST_WAIT);
}
