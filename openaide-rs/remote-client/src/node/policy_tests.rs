// timing-file: data — the rules take explicit durations; nothing waits.

use super::*;

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn only_an_app_hidden_long_enough_starts_over() {
    let timings = Timings::default();
    assert!(!timings.stale_after_hidden(Duration::ZERO));
    assert!(!timings.stale_after_hidden(timings.stale_after_hidden));
    assert!(timings.stale_after_hidden(timings.stale_after_hidden + ms(1)));
}

#[test]
fn a_failed_dial_keeps_the_endpoint_of_the_connection_in_use() {
    assert!(retire_after_failed_dial(false));
    assert!(!retire_after_failed_dial(true));
}

#[test]
fn a_stale_endpoint_waits_for_a_switch_dial_on_it() {
    assert!(retire_now(0));
    assert!(!retire_now(1));
}

#[test]
fn attempts_grow_and_stay_within_the_budget() {
    let timings = Timings::default();
    assert_eq!(timings.attempt_timeout(0, ms(0)), Some(ms(4_000)));
    assert_eq!(timings.attempt_timeout(1, ms(4_000)), Some(ms(8_000)));
    assert_eq!(timings.attempt_timeout(2, ms(12_000)), Some(ms(12_000)));
    assert_eq!(timings.attempt_timeout(3, ms(0)), None);
}

#[test]
fn an_attempt_that_failed_early_leaves_its_time_to_the_next() {
    // An endpoint replaced under a dial fails it at once; the next attempt is not shortened.
    assert_eq!(
        Timings::default().attempt_timeout(1, ms(50)),
        Some(ms(8_000))
    );
}

#[test]
fn the_last_attempt_ends_with_the_budget() {
    let timings = Timings::default();
    let budget = timings.connect_budget;
    assert_eq!(
        timings.attempt_timeout(2, budget - ms(5_000)),
        Some(ms(5_000))
    );
    assert_eq!(timings.attempt_timeout(2, budget - ms(999)), None);
    assert_eq!(timings.attempt_timeout(1, budget + ms(1)), None);
}

#[test]
fn only_the_first_attempt_uses_the_remembered_relay() {
    let relay = "https://relay.example./";
    assert_eq!(relay_for(0, Some(relay)), Some(relay));
    assert_eq!(relay_for(1, Some(relay)), None);
    assert_eq!(relay_for(0, None), None);
}

#[test]
fn a_stored_relay_that_is_not_an_address_is_ignored() {
    assert_eq!(relay_for(0, Some("")), None);
    assert_eq!(relay_for(0, Some("http://relay.example./")), None);
    assert_eq!(relay_for(0, Some("https://relay.example./\nnext")), None);
    let long = format!("https://{}", "a".repeat(200));
    assert_eq!(relay_for(0, Some(&long)), None);
}
